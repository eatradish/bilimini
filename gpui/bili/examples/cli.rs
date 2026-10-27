use bilimini_bili::Bili;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Bili::new()?;
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("feed") => {
            for video in client.feed().await? {
                println!(
                    "{} | {} | {}s | {} views | {}",
                    video.bvid,
                    video.title.chars().take(34).collect::<String>(),
                    video.duration,
                    video.views,
                    video.up
                );
            }
        }
        Some("search") => {
            let query = args.next().unwrap_or_default();
            for video in client.search_videos(&query).await? {
                println!(
                    "{} | {} | {}s | {}",
                    video.bvid,
                    video.title.chars().take(34).collect::<String>(),
                    video.duration,
                    video.up
                );
            }
        }
        Some("users") => {
            let query = args.next().unwrap_or_default();
            for user in client.search_users(&query).await? {
                println!("{} | {} | {} fans | {}", user.mid, user.name, user.fans, user.sign);
            }
        }
        Some("play") => {
            let bvid = args.next().expect("bvid");
            let info = client.play(&bvid, 0, 80).await?;
            println!(
                "{} cid={} qn={} {}x{:?} video={}B audio={}B",
                info.bvid,
                info.cid,
                info.quality,
                info.width.unwrap_or(0),
                info.height,
                info.video_url.len(),
                info.audio_url.len()
            );
        }
        _ => {
            eprintln!("usage: cli feed | search <q> | users <q> | play <bvid>");
        }
    }
    Ok(())
}
