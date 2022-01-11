use args::Args;
use client::DataMiner;
use models::User;

mod models;
mod client;
mod errors;
mod args;

type DataMResult<T> = Result<T, crate::errors::Error>;

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = argh::from_env::<Args>();
    let user = User::new(args.username, args.password);

    DataMiner::exec(user, args.is_soa).await;
}
