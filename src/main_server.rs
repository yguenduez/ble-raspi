//! Serves a Bluetooth GATT echo server.

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
use std::str::FromStr;
use std::time::Duration;
use systemstat::Platform;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    time,
    time::sleep,
};

const TEMPERATURE: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0001);
const CHARACTERISTIC_UUID: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0002);

#[tokio::main]
async fn main() -> bluer::Result<()> {
    env_logger::init();
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;
    let service_uuid: uuid::Uuid =
        uuid::Uuid::from_str("FD2B4448-AA0F-4A15-A62F-EB0BE77A0000").unwrap();

    println!(
        "Advertising on Bluetooth adapter {} with address {}",
        adapter.name(),
        adapter.address().await?
    );
    let le_advertisement = Advertisement {
        service_uuids: vec![service_uuid].into_iter().collect(),
        discoverable: Some(true),
        local_name: Some("gatt_echo_server".to_string()),
        ..Default::default()
    };
    let adv_handle = adapter.advertise(le_advertisement).await?;

    println!(
        "Serving GATT echo service on Bluetooth adapter {}",
        adapter.name()
    );
    let (write_notify_control, write_notify_handle) = characteristic_control();
    let (temp_control, temp_handle) = characteristic_control();
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
                    control_handle: write_notify_handle,
                    ..Default::default()
                },
                Characteristic {
                    uuid: TEMPERATURE,
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: temp_handle,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;

    println!("Echo service ready. Press enter to quit.");
    let stdin = BufReader::new(tokio::io::stdin());

    let mut read_buf = Vec::new();
    let mut reader_opt: Option<CharacteristicReader> = None;
    let mut writer_opt: Option<CharacteristicWriter> = None;

    let mut temp_writer_opt: Option<CharacteristicWriter> = None;

    pin_mut!(write_notify_control);
    pin_mut!(temp_control);

    let sys = systemstat::System::new();

    loop {
        tokio::select! {
            evt = write_notify_control.next() => {
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
            evt = temp_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {}", notifier.mtu());
                        temp_writer_opt = Some(notifier);
                    }
                    None => break,
                    _ => break,
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
            },
            _ = time::sleep(Duration::from_secs(1)) => {
                let cpu_load = sys.cpu_load_aggregate()?.done()?;
                let system_cpu_load = cpu_load.system;
                let cpu_temperature = sys.cpu_temp()?;
                let memory_usage = sys.memory()?;
                let uptime = sys.uptime()?;
                let uptime_minutes = uptime.as_secs()/60;

                if let Some(writer) = &mut temp_writer_opt {
                    writer.write_f32(cpu_temperature).await?;
                    println!("Updated CPU Temperature: {:.2}%", cpu_temperature);
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
