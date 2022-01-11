use std::{sync::Arc, time::Duration};

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use log::{debug, error, info};
use reqwest::{
    header::{HeaderMap, ACCEPT, USER_AGENT},
    Client, Response, Url,
};
use scraper::Html;
use serde::Serialize;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{
    errors,
    models::{auth_parser, Gym, GymSlotData, GymSlotDataSoA, LoginCredentials, Timeslot, User},
    DataMResult,
};

#[derive(Clone, Debug)]
pub struct DataMiner {
    internal_client: Client,
}

impl Default for DataMiner {
    fn default() -> Self {
        let mut headers = HeaderMap::new();
        headers.append(USER_AGENT, Self::USER_AGENT.parse().unwrap());
        headers.append(ACCEPT, Self::ACCEPT_HEADER.parse().unwrap());
        Self {
            internal_client: Client::builder()
                .default_headers(headers)
                .cookie_store(true)
                .build()
                .unwrap(),
        }
    }
}

impl DataMiner {
    const USER_AGENT: &'static str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:95.0) Gecko/20100101 Firefox/95.0";

    const ACCEPT_HEADER: &'static str =
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";

    pub async fn exec(user: User, is_soa: bool) {
        // 20 min interval
        let mut interval_timer = tokio::time::interval(Duration::from_secs(60 * 20));
        let user = Arc::new(user);

        loop {
            // wait for next tick
            interval_timer.tick().await;

            let user = user.clone();
            let dt = [
                (Utc::now().naive_local()).date(),
                (Utc::now().naive_local() + chrono::Duration::days(2)).date(),
                (Utc::now().naive_local() + chrono::Duration::days(3)).date(),
            ];

            tokio::spawn(async move {
                for gym in Gym::gym_slice() {
                    for d in dt {
                        let data_miner = DataMiner::default();
                        if is_soa {
                            if let Err(e) = data_miner
                                .get_slots::<_, GymSlotDataSoA>(&user, *gym, d)
                                .await
                            {
                                error!("{}", e);
                            }
                        } else {
                            if let Err(e) =
                                data_miner.get_slots::<_, GymSlotData>(&user, *gym, d).await
                            {
                                error!("{}", e);
                            }
                        }
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            });
        }
    }

    async fn write_to_file<T>(buf: T, gym: Gym) -> DataMResult<()>
    where
        T: Serialize,
    {
        let now = Utc::now().naive_local();
        let with_tz = DateTime::<FixedOffset>::from_utc(now, FixedOffset::east(3600 * 8));
        let dt_str = with_tz.format("%Y-%m-%d %H-%M-%S").to_string();
        let dt_no_time = with_tz.format("%Y-%m-%d").to_string();

        let dir_out = tokio::fs::create_dir(format!("output/{}", &dt_no_time)).await;

        if let Err(e) = dir_out {
            match e.kind() {
                std::io::ErrorKind::AlreadyExists => (),
                _ => return Err(errors::Error::Io(e))
           }
        }

        let filename = format!("output/{}/{:?}-{}.json", dt_no_time, gym, dt_str);

        let data = serde_json::to_string_pretty(&buf).unwrap();

        let mut f = File::create(&filename).await?;
        f.write(data.as_bytes()).await?;

        info!("{}, write successful", filename);
        Ok(())
    }

    async fn get_slots<D, T>(&self, user: &User, gym: Gym, date: D) -> DataMResult<()>
    where
        D: Into<NaiveDate>,
        T: Serialize + From<GymSlotData>,
    {
        let login = self.login(user).await?;
        let referer_url = login.url();

        let res = self.query_timeslots(referer_url, gym, date).await?;

        debug!("{:?}", &res);
        let data = GymSlotData::new(gym, Utc::now().naive_utc(), res);
        let data = Into::into(data);
        let _ = Self::write_to_file::<T>(data, gym).await?;

        Ok(())
    }

    /// Example query
    /// `https://members.myactivesg.com/facilities/view/activity/1031/venue/154?time_from=1616256000`
    ///
    /// Returns the timeslots and parsed Html of the page
    async fn query_timeslots<D, S>(
        &self,
        referer_url: S,
        gym_id: Gym,
        date: D,
    ) -> Result<Vec<Timeslot>, errors::Error>
    where
        D: Into<NaiveDate>,
        S: AsRef<str>,
    {
        let facility_type = 1031u32;
        let date = date.into();

        let date_timestamp = date.and_hms(0, 0, 0).timestamp();

        // this API does not work when it is 0600 - 0800
        let url = Url::parse(&format!(
            "https://members.myactivesg.com/facilities/view/activity/{}/venue/{}?time_from={}",
            facility_type, gym_id as u16, date_timestamp
        ))
        .map_err(|_| errors::Error::FailedToParseUrl)?;

        let res = self
            .internal_client
            .get(url)
            .header("Referer", referer_url.as_ref())
            .send()
            .await?;

        let body = res.text().await?;
        let html = Html::parse_document(&body);

        Ok(Timeslot::parse_timeslots(&html, date))
    }

    fn handle_login_credentials(body: String, user: &User) -> DataMResult<LoginCredentials> {
        let html = Html::parse_document(&body);
        let csrf_token = auth_parser::get_csrf_token(&html)?;
        let rsa_key = auth_parser::get_rsa_key(&html)?;

        let enc_pwd = auth_parser::generate_enc_pwd(&rsa_key, &user.password)?;

        Ok(LoginCredentials::new(
            user.email.clone(),
            enc_pwd,
            csrf_token,
        ))
    }

    /// Logins using user provided
    async fn login(&self, user: &User) -> DataMResult<Response> {
        let login_url = "https://members.myactivesg.com/auth";
        let sign_in = "https://members.myactivesg.com/auth/signin";

        let resp_builder = self
            .internal_client
            .get(login_url)
            .header(USER_AGENT, Self::USER_AGENT)
            .header(ACCEPT, Self::ACCEPT_HEADER);

        debug!("{:X?}", &resp_builder);

        let resp = resp_builder.send().await?;

        info!("GET login page successful!");

        let body = resp.text().await?;

        let login_creds = Self::handle_login_credentials(body, user)?;

        let login = self
            .internal_client
            .post(sign_in)
            .header(USER_AGENT, Self::USER_AGENT)
            .header(ACCEPT, Self::ACCEPT_HEADER)
            .form(&login_creds)
            .send()
            .await?;

        info!("POST login successful!");

        match login.url().as_str() {
            "https://members.myactivesg.com/profile" => {
                info!("Logged in successfully!");
                Ok(login)
            }
            _ => Err(errors::Error::InvalidCredentialsSessionExpired),
        }
    }
}
