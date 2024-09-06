use crate::error::{Error, SerialError};
use crate::ScreenData;
use bytes::{BufMut, BytesMut};
use futures::stream::StreamExt;
use futures::SinkExt;
use serde::de::DeserializeOwned;
use serialport::SerialPortInfo;
use std::{io, str};
use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, SerialStream, StopBits};
use tokio_util::codec::Framed;
use tokio_util::codec::{Decoder, Encoder};

use crate::protocol::{Command, Message, SettingsRoot};

type R<T> = std::result::Result<T, SerialError>;

pub async fn connect(device: AvailableDevice) -> Result<Device, Error> {
    println!("Connecting to device: {:?}", device.port_info.port_name);
    let mut port = tokio_serial::new(device.port_info.port_name, 115200)
        .data_bits(DataBits::Eight)
        .stop_bits(StopBits::One)
        .parity(Parity::None)
        .flow_control(FlowControl::None)
        .open_native_async()?;

    #[cfg(unix)]
    port.set_exclusive(false)
        .expect("Unable to set serial port exclusive to false");

    Ok(Device::create_async(port))
}

pub fn discover() -> Result<Vec<AvailableDevice>, Error> {
    let ports = Device::enumerate()?;

    if ports.is_empty() {
        return Err(Error::NoDevicesFound);
    }

    let devices: Vec<AvailableDevice> = ports
        .iter()
        .map(|p| AvailableDevice {
            port_info: p.clone(),
        })
        .collect();

    Ok(devices)
}

#[derive(Debug)]
pub struct AvailableDevice {
    port_info: serialport::SerialPortInfo,
}

#[derive(Debug)]
pub struct Device {
    message_pipe: tokio::sync::broadcast::Sender<R<Message>>,
    command_pipe: tokio::sync::mpsc::Sender<Command>,
}

impl Device {
    fn enumerate() -> Result<Vec<SerialPortInfo>, Error> {
        let ports = serialport::available_ports()?;

        let res: Vec<SerialPortInfo> = ports
            .iter()
            .filter(|p| match &p.port_type {
                serialport::SerialPortType::UsbPort(port) => port.vid == 12346 && port.pid == 4097,
                _ => false,
            })
            .filter(|p| p.port_name.starts_with("/dev/tty"))
            .cloned()
            .collect();

        Ok(res)
    }

    async fn command_handler<R>(&mut self, value: Command) -> R
    where
        R: DeserializeOwned + Send + 'static,
    {
        let mut r = self.message_pipe.subscribe();
        let _ = self.command_pipe.send(value).await;

        loop {
            let message = r.recv().await.unwrap();
            dbg!(&message);
            match message {
                Ok(msg) => {
                    if let Ok(result) = serde_json::from_value(serde_json::to_value(&msg).unwrap())
                    {
                        return result;
                    } else {
                    }
                }
                Err(t) => {
                    dbg!(t);
                }
            }
        }
    }

    pub async fn show_message(
        &mut self,
        title: Option<String>,
        text: Option<String>,
        duration: Option<f32>,
    ) -> Result<(), Error> {
        let msg = crate::protocol::MessageDetails {
            title,
            text,
            duration,
        };
        self.command_pipe
            .send(Command::ShowMessage(msg))
            .await
            .unwrap();
        Ok(())
    }

    pub async fn set_screen(&mut self, data: ScreenData) -> Result<(), Error> {
        self.command_pipe
            .send(Command::SetScreen(data))
            .await
            .unwrap();
        Ok(())
    }

    pub async fn get_settings(&mut self) -> crate::protocol::Settings {
        let v: SettingsRoot = self.command_handler(Command::GetSettings).await;

        v.settings
    }

    pub async fn get_profiles(&mut self) -> Option<Vec<String>> {
        let v: crate::protocol::Profiles = self.command_handler(Command::GetProfiles).await;

        v.profiles
    }

    // pub async fn get_current_profile(&mut self) -> Option<String> {
    //     self.get_profiles()
    // }

    pub async fn get_profile(&mut self, name: String) -> crate::protocol::Profile {
        let r: crate::protocol::Profile = self.command_handler(Command::GetProfile(name)).await;

        r
    }

    fn create_async(port: SerialStream) -> Device {
        let mut framed = Framed::new(port, LineCodec {});

        let (message_pipe, _) = tokio::sync::broadcast::channel(50);
        let (command_pipe, mut cr): (tokio::sync::mpsc::Sender<Command>, _) =
            tokio::sync::mpsc::channel(50);
        let mp = message_pipe.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                        // incoming
                    res = framed.next() => {
                        match res {
                            Some(Ok(line)) => {
                                if let Ok(message) = crate::protocol::Message::try_from(line.as_str()) {
                                    let _ = mp.send(Ok(message));
                                } else {
                                    let _ = mp.send(Err(SerialError::UnhandledMessage(line)));
                                }
                            }
                            Some(Err(e)) => {
                                let _ = mp.send(Err(SerialError::ErrorReading(e.to_string())));
                                break;
                            }
                            None => {
                                let _ = mp.send(Err(SerialError::SerialPortClosed));
                                break;
                            }
                        }
                    }
                    cmd = cr.recv() => {
                        match cmd {
                            Some(c) => {
                                let _ = framed.send(c.to_string()).await;
                            }
                            None => {
                                break;
                            }
                        }
                    }

                }
            }
        });

        Device {
            message_pipe,
            command_pipe,
        }
    }

    pub fn subscribe(&mut self) -> tokio::sync::broadcast::Receiver<R<crate::protocol::Message>> {
        self.message_pipe.subscribe()
    }

    pub fn events(
        &mut self,
    ) -> tokio::sync::mpsc::Receiver<Result<crate::protocol::Message, Error>> {
        let (tx, rx) = tokio::sync::mpsc::channel(50);
        let mut upstream = self.message_pipe.subscribe();

        tokio::spawn(async move {
            loop {
                let message = upstream.recv().await;

                match message {
                    Ok(Ok(m)) => match m {
                        Message::Event(_) => {
                            let _ = tx.send(Ok(m)).await;
                        }
                        _ => {}
                    },
                    Ok(Err(e)) => {
                        let _ = tx.send(Err(Error::SerialError(e))).await;
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        rx
    }

    pub async fn command(&mut self, command: Command) -> Result<(), Error> {
        self.command_pipe.send(command).await.unwrap();
        Ok(())
    }
}

#[derive(Debug)]
struct LineCodec {}

impl Decoder for LineCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let newline = src.as_ref().iter().position(|b| *b == b'\n');
        if let Some(n) = newline {
            let line = src.split_to(n + 1);
            return match str::from_utf8(line.as_ref()) {
                Ok(s) => Ok(Some(s.to_string())),
                Err(_) => Err(io::Error::new(io::ErrorKind::Other, "Invalid String")),
            };
        }
        Ok(None)
    }
}

impl Encoder<String> for LineCodec {
    type Error = io::Error;

    fn encode(&mut self, item: String, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.reserve(item.len());
        dst.put(item.as_bytes());
        dst.put_u8(b'\n');
        Ok(())
    }
}
