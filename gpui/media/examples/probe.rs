fn main() {
    let target = std::env::args().nth(1).expect("usage: probe <video>");
    let player = bilimini_media::Player::open(&target, 640, 360);
    let start = std::time::Instant::now();
    let mut count = 0;
    while start.elapsed().as_secs() < 5 {
        if let Some(frame) = player.try_frame() {
            count += 1;
            if count == 1 || count % 30 == 0 {
                let sum: u64 = frame.data.iter().map(|b| *b as u64).sum();
                let max = frame.data.iter().copied().max().unwrap_or(0);
                let avg = sum / frame.data.len().max(1) as u64;
                println!(
                    "frame {} seq {} size {}x{} avg={} max={} px0={},{},{}",
                    count, frame.seq, frame.width, frame.height, avg, max,
                    frame.data[0], frame.data[1], frame.data[2]
                );
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("total frames: {count}");
}
