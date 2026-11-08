fn main() -> anyhow::Result<()> {
    println!("inputs:");
    for device in dex_audio::device_choices(true)? {
        println!("  {} — {}", device.id, device.label);
    }
    println!("outputs:");
    for device in dex_audio::device_choices(false)? {
        println!("  {} — {}", device.id, device.label);
    }
    Ok(())
}
