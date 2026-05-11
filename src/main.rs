<<<<<<< ours
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let _logging = match vzdc_discord_bot::logging::init() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = vzdc_discord_bot::run().await {
        tracing::error!(?error, "application terminated with error");
        std::process::exit(1);
    }
}
|||||||
=======
fn main() {
    println!("Hello, world!");
}
>>>>>>> theirs
