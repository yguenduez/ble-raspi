//! Serves a Bluetooth GATT echo server.

/// Service UUID for GATT example.
///
const SERVICE_ID: &str = "FD2B4448-AA0F-4A15-A62F-EB0BE77A0000";

/// Characteristic UUID for GATT example.
const CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xF00DC0DE00002);

/// RSSI Characteristic UUID for GATT example.
const RSSI_CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xFEEDC0DE00003);

use std::str::FromStr;
use bluer::{
    adv::Advertisement,
    gatt::{
        local::{
            characteristic_control, Application, Characteristic, CharacteristicControlEvent,
            CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicWrite,
            CharacteristicWriteMethod, Service,
        },
        CharacteristicReader, CharacteristicWriter,
    },
};
use futures::{future, pin_mut, StreamExt};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    time::sleep,
};

#[tokio::main]
async fn main() -> bluer::Result<()> {
    let service_uuid =  uuid::Uuid::from_str(&SERVICE_ID.to_lowercase()).unwrap();
    env_logger::init();
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    println!(
        "Advertising on Bluetooth adapter {} with address {}",
        adapter.name(),
        adapter.address().await?
    );
    let le_advertisement = Advertisement {
        service_uuids: vec![service_uuid.clone()].into_iter().collect(),
        discoverable: Some(true),
        local_name: Some("gatt_echo_server".to_string()),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(le_advertisement).await?;

    println!(
        "Serving GATT echo service on Bluetooth adapter {}",
        adapter.name()
    );
    let (char_control, char_handle) = characteristic_control();
    let app = Application {
        services: vec![Service {
            uuid: service_uuid,
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: CHARACTERISTIC_UUID,
                    write: Some(CharacteristicWrite {
                        write_without_response: true,
                        method: CharacteristicWriteMethod::Io,
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: char_handle,
                    ..Default::default()
                },
                // Add RSSI Characteristic
                Characteristic {
                    uuid: RSSI_CHARACTERISTIC_UUID,
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;

    println!("GATT Service Ready - Serving");

    let mut read_buf = Vec::new();
    let mut reader_opt: Option<CharacteristicReader> = None;
    let mut writer_opt: Option<CharacteristicWriter> = None;
    pin_mut!(char_control);

    loop {
        let rssi_value: i32 = 42;
        let rssi_data = rssi_value.to_le_bytes();

        if let Some(writer) = &mut writer_opt {
            writer.write_all(&rssi_data).await?;
        }

        tokio::select! {
            evt = char_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Write(req)) => {
                        println!("Accepting write request event with MTU {}", req.mtu());
                        read_buf = vec![0; req.mtu()];
                        reader_opt = Some(req.accept()?);
                    },
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {}", notifier.mtu());
                        writer_opt = Some(notifier);
                    },
                    None => break,
                }
            },
            read_res = async {
                match &mut reader_opt {
                    Some(reader) if writer_opt.is_some() => reader.read(&mut read_buf).await,
                    _ => future::pending().await,
                }
            } => {
                match read_res {
                    Ok(0) => {
                        println!("Read stream ended");
                        reader_opt = None;
                    }
                    Ok(n) => {
                        let value = read_buf[..n].to_vec();
                        println!("Echoing {} bytes: {:x?} ... {:x?}", value.len(), &value[0..4.min(value.len())], &value[value.len().saturating_sub(4) ..]);
                        if value.len() < 512 {
                            println!();
                        }
                        if let Err(err) = writer_opt.as_mut().unwrap().write_all(&value).await {
                            println!("Write failed: {}", &err);
                            writer_opt = None;
                        }
                    }
                    Err(err) => {
                        println!("Read stream error: {}", &err);
                        reader_opt = None;
                    }
                }
            }
        }
    }

    println!("Removing service and advertisement");
    drop(app_handle);
    drop(adv_handle);
    sleep(Duration::from_secs(1)).await;

    Ok(())
}
