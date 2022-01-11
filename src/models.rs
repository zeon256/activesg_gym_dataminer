use crate::errors;
use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc, NaiveDateTime};
use lazy_static::lazy_static;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

lazy_static! {

    /// Regex for slot count
    ///
    /// ## Example of slot count
    /// - 0 Left
    /// - 50 Left
    pub static ref SLOT_RE: Regex = Regex::new("([0-9]+) Left").unwrap();

    /// Regex for the slot timings
    ///
    /// ## Example of slot timings
    /// - 07:00 AM
    /// - 07:00 PM
    /// - 11:00 PM
    pub static ref TIME_RE: Regex = Regex::new("([0-9]+):[0-9]+ ([PM|AM])").unwrap();
}

pub mod auth_parser {
    use crate::{errors, DataMResult};
    use openssl::rsa::Padding;
    use scraper::{Html, Selector};

    pub fn get_rsa_key(body: &Html) -> DataMResult<String> {
        let rsa_key_selector = Selector::parse(r#"input[name="rsapublickey"]"#)
            .map_err(|_| errors::Error::CantFindElement("rsapublickey"))?;

        body.select(&rsa_key_selector)
            .next()
            .map(|v| v.value())
            .and_then(|v| v.attr("value"))
            .ok_or(errors::Error::CantFindElement("rsapublickey"))
            .map(|s| s.to_string())
    }

    pub fn get_csrf_token(body: &Html) -> DataMResult<String> {
        let csrf_token_selector = Selector::parse(r#"input[name="_csrf"]"#)
            .map_err(|_| errors::Error::CantFindElement("_csrf"))?;

        body.select(&csrf_token_selector)
            .next()
            .map(|v| v.value())
            .and_then(|v| v.attr("value"))
            .ok_or(errors::Error::CantFindElement("_csrf"))
            .map(|s| s.into())
    }

    pub fn generate_enc_pwd(public_key: &str, pwd_raw: &str) -> DataMResult<String> {
        use openssl::rsa;

        let p_key = rsa::Rsa::public_key_from_pem(public_key.as_bytes())
            .map_err(|_| errors::Error::FailedToParsePEM)?;

        let mut buf = vec![0u8; p_key.size() as usize];

        p_key
            .public_encrypt(pwd_raw.as_bytes(), &mut buf, Padding::PKCS1)
            .map_err(|_| errors::Error::FailedToGenerateKeyFromPEM)?;

        Ok(base64::encode(buf))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct User {
    /// email address of the user
    pub email: String,

    /// user's password
    pub password: String,
}

impl User {
    pub fn new<S: Into<String>>(email_address: S, password: S) -> Self {
        Self {
            email: email_address.into(),
            password: password.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LoginCredentials {
    email: String,
    ecpassword: String,
    _csrf: String,
}

impl LoginCredentials {
    pub fn new<S: Into<String>>(email: S, ecpassword: S, _csrf: S) -> Self {
        LoginCredentials {
            email: email.into(),
            ecpassword: ecpassword.into(),
            _csrf: _csrf.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GymSlotDataSoA {
    gym: Gym,
    datetime: NaiveDateTime,
    time: Vec<DateTime<Utc>>,
    slots_avail: Vec<u8>
}

impl From<GymSlotData> for GymSlotDataSoA {
    fn from(data: GymSlotData) -> Self {
        let mut time = vec![];
        let mut slots_avail = vec![];

        for t in data.data {
            time.push(t.time);
            slots_avail.push(t.slots_avail);
        }

        Self {
            gym: data.gym,
            datetime: data.datetime,
            time,
            slots_avail 
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GymSlotData {
    gym: Gym,
    datetime: NaiveDateTime,
    data: Vec<Timeslot>
}

impl GymSlotData {
    pub fn new(gym: Gym, datetime: NaiveDateTime, data: Vec<Timeslot>) -> Self {
        Self {
            gym,
            datetime,
            data
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timeslot {
    time: DateTime<Utc>,
    slots_avail: u8,
}

/// Unchecked DateTime that is on the the webpage,
#[derive(Debug, Copy, Clone)]
pub struct ActiveSgDatetime<'a> {
    /// This string contains all the necessary information to parse into a DateTime<FixedOffset>
    unchecked_string: &'a str,

    date: NaiveDate,
}

/// Checked number of slots, which internally uses u8
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActiveSgSlotCount(pub u8);

impl TryFrom<&str> for ActiveSgSlotCount {
    type Error = errors::Error;

    /// Try to parse the slot count based on the string provided.
    ///
    /// Usually the string provided is the html page itself
    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        if SLOT_RE.is_match(value) {
            let caps = SLOT_RE.captures(&value).unwrap();
            caps.get(1)
                .map(|m| m.as_str())
                .and_then(|m| m.parse::<u8>().ok())
                .map(|m| ActiveSgSlotCount(m))
                .ok_or(errors::Error::CantFindElement("Missing slot no!"))
        } else {
            Err(errors::Error::CantFindElement("Missing slot no!"))
        }
    }
}

impl<'a> ActiveSgDatetime<'a> {
    /// In this case, day must have time of 0000
    pub fn new(unchecked_string: &'a str, date: NaiveDate) -> Self {
        Self {
            unchecked_string,
            date,
        }
    }
}

impl TryFrom<ActiveSgDatetime<'_>> for DateTime<Utc> {
    type Error = errors::Error;

    /// Try to convert [ActiveSgDatetime] to DateTime<Utc>
    ///
    /// 1 Assumption is made, that the data in [ActiveSgDatetime]
    fn try_from(value: ActiveSgDatetime<'_>) -> Result<Self, Self::Error> {
        if TIME_RE.is_match(value.unchecked_string) {
            let caps = TIME_RE.captures(value.unchecked_string).unwrap();

            // Match for time portion of text
            let time = caps
                .get(1)
                .map(|m| m.as_str())
                .and_then(|m| m.parse::<u32>().ok());

            // Match for AM/PM part of text
            // matches only for A or P
            let am_pm = caps.get(2).map(|m| m.as_str());

            match (time, am_pm) {
                (Some(t), Some(m)) => {
                    let t = match m {
                        "P" => t + 12, // adds 12 hours
                        "A" => t,
                        _ => return Err(errors::Error::CantFindElement("Cant find timeslot!")),
                    };

                    let t = NaiveTime::from_hms(t, 0, 0);

                    // Minus 8 hours because the user interface on activesg
                    // website is in GMT+8
                    // and we want to store in utc
                    let dt = value
                        .date
                        .and_time(t)
                        .checked_sub_signed(chrono::Duration::hours(8))
                        .unwrap();

                    Ok(Utc.from_utc_datetime(&dt))
                }
                _ => Err(errors::Error::CantFindElement("Cant find timeslot!")),
            }
        } else {
            Err(errors::Error::CantFindElement("Cant find timeslot!"))
        }
    }
}
impl Timeslot {
    pub fn new(time: DateTime<Utc>, slots_avail: u8) -> Self {
        Timeslot { time, slots_avail }
    }

    pub fn mut_slots_avail(&mut self, slots_avail: u8) {
        self.slots_avail = slots_avail;
    }

    pub fn mut_time(&mut self, time: DateTime<Utc>) {
        self.time = time;
    }

    /// Parses the timeslots from the booking page html file
    /// and collets it to a [Vec<Timeslot>]
    ///
    /// This method is infallible and will return an empty [Vec<Timeslot>] if nothing is added to it
    pub fn parse_timeslots(body: &Html, day: NaiveDate) -> Vec<Timeslot> {
        let mut buf = Vec::with_capacity(15);
        let timeslot_selector = Selector::parse(".chkbox-grid").unwrap();
        let label_selector = Selector::parse("label").unwrap();

        // dummy buffer which will get filled based on the string
        let mut timeslot = Timeslot::new(Utc::now(), 0);

        for item in body.select(&timeslot_selector) {
            let html = Html::parse_document(&item.html());

            // This will iterate the labels like this
            // 07:00 AM
            // 25 Left
            // etc...
            for label in html.select(&label_selector) {
                let text = label.text().collect::<String>();
                let slot_count = ActiveSgSlotCount::try_from(text.as_str());
                let asg_dt = ActiveSgDatetime::new(&text, day);
                let dt = DateTime::try_from(asg_dt);

                if let Ok(time) = dt {
                    timeslot.mut_time(time);
                }

                if let Ok(slot) = slot_count {
                    timeslot.mut_slots_avail(slot.0);
                    buf.push(timeslot.clone());
                }
            }
        }

        buf
    }
}

#[allow(non_camel_case_types, unused)]
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Gym {
    AMK_CC = 1016,
    FERNVALE_SQ = 1048,
    TOA_PAYOH_CC = 1049,
    HOKEY_VILLAGE_BOONLAY = 1037,
    BISHAN = 137,
    BUKIT_BATOK = 1040,
    BUKIT_GOMBAK = 145,
    CHOA_CHU_KANG = 154,
    CLEMENTI = 160,
    ENABLING_VILLAGE = 849,
    HEARTBEAT_BEDOK = 896,
    HOUGANG = 185,
    JALAN_BESAR = 967,
    JURONG_EAST = 196,
    JURONG_LAKE = 1012,
    JURONG_WEST = 200,
    PASIR_RIS = 544,
    SENGKANG = 239,
    SENJA_CASHEW = 1089,
    SILVER_CIRCLE = 886,
    TAMPINES = 900,
    TOA_PAYOH = 268,
    WOODLANDS = 274,
    YIO_CHU_KANG = 279,
    YISHUN = 284,
}

impl Gym {
    pub const fn gym_slice() -> &'static [Self] {
        &[
            Gym::AMK_CC,
            Gym::FERNVALE_SQ,
            Gym::TOA_PAYOH_CC,
            Gym::HOKEY_VILLAGE_BOONLAY,
            Gym::BISHAN,
            Gym::BUKIT_BATOK,
            Gym::BUKIT_GOMBAK,
            Gym::CHOA_CHU_KANG,
            Gym::CLEMENTI,
            Gym::ENABLING_VILLAGE,
            Gym::HEARTBEAT_BEDOK,
            Gym::HOUGANG,
            Gym::JALAN_BESAR,
            Gym::JURONG_EAST,
            Gym::JURONG_LAKE,
            Gym::JURONG_WEST,
            Gym::PASIR_RIS,
            Gym::SENGKANG,
            Gym::SENJA_CASHEW,
            Gym::SILVER_CIRCLE,
            Gym::TAMPINES,
            Gym::TOA_PAYOH,
            Gym::WOODLANDS,
            Gym::YIO_CHU_KANG,
            Gym::YISHUN,
        ]
    }
}

impl FromStr for Gym {
    type Err = errors::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "AMK_CC" => Ok(Gym::AMK_CC),
            "FERNVALE_SQ" => Ok(Gym::FERNVALE_SQ),
            "TOA_PAYOH_CC" => Ok(Gym::TOA_PAYOH_CC),
            "HOKEY_VILLAGE_BOONLAY" => Ok(Gym::HOKEY_VILLAGE_BOONLAY),
            "BISHAN" => Ok(Gym::BISHAN),
            "BUKIT_BATOK" => Ok(Gym::BUKIT_BATOK),
            "BUKIT_GOMBAK" => Ok(Gym::BUKIT_GOMBAK),
            "CHOA_CHU_KANG" => Ok(Gym::CHOA_CHU_KANG),
            "CLEMENTI" => Ok(Gym::CLEMENTI),
            "ENABLING_VILLAGE" => Ok(Gym::ENABLING_VILLAGE),
            "HEARTBEAT_BEDOK" => Ok(Gym::HEARTBEAT_BEDOK),
            "HOUGANG" => Ok(Gym::HOUGANG),
            "JALAN_BESAR" => Ok(Gym::JALAN_BESAR),
            "JURONG_EAST" => Ok(Gym::JURONG_EAST),
            "JURONG_LAKE" => Ok(Gym::JURONG_LAKE),
            "JURONG_WEST" => Ok(Gym::JURONG_WEST),
            "PASIR_RIS" => Ok(Gym::PASIR_RIS),
            "SENGKANG" => Ok(Gym::SENGKANG),
            "SENJA_CASHEW" => Ok(Gym::SENJA_CASHEW),
            "SILVER_CIRCLE" => Ok(Gym::SILVER_CIRCLE),
            "TAMPINES" => Ok(Gym::TAMPINES),
            "TOA_PAYOH" => Ok(Gym::TOA_PAYOH),
            "WOODLANDS" => Ok(Gym::WOODLANDS),
            "YIO_CHU_KANG" => Ok(Gym::YIO_CHU_KANG),
            "YISHUN" => Ok(Gym::YISHUN),
            _ => Err(errors::Error::InvalidGym(s.into())),
        }
    }
}
