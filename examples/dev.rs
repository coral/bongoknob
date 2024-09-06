use bongoknob;

use anyhow::Result;

fn main() -> Result<()> {
    let devices = bongoknob::discover()?;
    for device in devices {
        println!("Found device: {}", device);

        let device = bongoknob::connect(device)?;

        let settings = device.get_settings()?;
        dbg!(&settings);

        let profiles = device.get_profiles()?;
        dbg!(&profiles);

        device
            .set_message(
                Some("Helloooo".to_string()),
                Some("wohoooooooo".to_string()),
                Some(599999.0),
            )
            .unwrap();

        // update some settings
        device.set_settings(bongoknob::Settings {
            led_max_brightness: Some(150),
            device_orientation: Some(1),
            ..Default::default()
        })?;

        // subscribe to messages
        let pipe = device.subscribe();

        loop {
            let message = pipe.recv_timeout(std::time::Duration::from_secs(10))?;
            println!("Received message: {:?}", message);
        }
    }

    Ok(())
}
