use systemstat::{Platform, System};

const SERVICE_ID: &str = "FD2B4448-AA0F-4A15-A62F-EB0BE77A0000";

/// Temperature
const TEMPERATURE: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0001);

/// CPU LOAD
const CPU_LOAD: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0002);

/// RAM USAGE
const RAM_USAGE: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0003);

/// Uptime
const UPTIME: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0004);

/// Request Response
const WRITE_REQUEST_RESPONSE: uuid::Uuid = uuid::Uuid::from_u128(0xfd2bcccb0005);

use bluer::gatt::local::{CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, CharacteristicWriteRequest};
use bluer::{
    adv::Advertisement,
    gatt::{
        local::{
            characteristic_control, Application, Characteristic, CharacteristicControlEvent,
            CharacteristicNotify, CharacteristicNotifyMethod, Service,
        },
        CharacteristicWriter,
    },
};
use futures::{future, pin_mut, StreamExt};
use std::str::FromStr;
use std::time::Duration;
use bluer::gatt::CharacteristicReader;
use tokio::{io::AsyncWriteExt, time, time::sleep};
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> bluer::Result<()> {
    let service_uuid = uuid::Uuid::from_str(&SERVICE_ID.to_lowercase()).unwrap();
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
    let (mut memory_control, memory_handle) = characteristic_control();
    let (cpu_control, cpu_handle) = characteristic_control();
    let (temp_control, temp_handle) = characteristic_control();
    let (uptime_control, uptime_handle) = characteristic_control();

    let (write_request_control, write_request_handle) = characteristic_control();

    let app = Application {
        services: vec![Service {
            uuid: service_uuid,
            primary: true,
            characteristics: vec![
                // CPU Load characteristic
                Characteristic {
                    uuid: CPU_LOAD,
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: cpu_handle,
                    ..Default::default()
                },
                // CPU Temperature
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
                // Memory Usage
                Characteristic {
                    uuid: RAM_USAGE,
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: memory_handle,
                    ..Default::default()
                },
                // Uptime Usage
                Characteristic {
                    uuid: UPTIME,
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: uptime_handle,
                    ..Default::default()
                },
                // Request Response characteristic (with write/notify)
                Characteristic {
                    uuid: WRITE_REQUEST_RESPONSE,
                    write: Some(CharacteristicWrite {
                        write_without_response: false,
                        method: CharacteristicWriteMethod::Io,
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        method: CharacteristicNotifyMethod::Io,
                        ..Default::default()
                    }),
                    control_handle: write_request_handle,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let app_handle = adapter.serve_gatt_application(app).await?;

    println!("GATT Service Ready - Serving");

    let mut cpu_load_writer_opt: Option<CharacteristicWriter> = None;
    let mut temp_writer_opt: Option<CharacteristicWriter> = None;
    let mut memory_writer_opt: Option<CharacteristicWriter> = None;
    let mut uptime_writer_opt: Option<CharacteristicWriter> = None;

    let mut write_opt: Option<CharacteristicWriter> = None;
    let mut read_opt: Option<CharacteristicReader> = None;

    pin_mut!(cpu_control);
    pin_mut!(temp_control);
    pin_mut!(memory_control);
    pin_mut!(uptime_control);
    pin_mut!(write_request_control);

    let mut read_buf = vec![];

    let sys = System::new();

    loop {
        tokio::select! {
             evt = write_request_control.next() => {
             match evt {
                Some(CharacteristicControlEvent::Write(req)) => {
                    println!("Accepting write request event with MTU {}", req.mtu());
                    read_buf = vec![0; req.mtu()];
                    read_opt = Some(req.accept()?);
                },
                Some(CharacteristicControlEvent::Notify(notifier)) => {
                    println!("Accepting notify request event with MTU {}", notifier.mtu());
                    write_opt = Some(notifier);
                },
                None => break,
                }
            },
            read_res = async {
                match &mut read_opt {
                    Some(reader) if write_opt.is_some() => reader.read(&mut read_buf).await,
                    _ => future::pending().await,
                }
            } => {
                match read_res {
                    Ok(0) => {
                        println!("Read stream ended");
                        read_opt = None;
                    }
                    Ok(n) => {
                        let value = read_buf[..n].to_vec();
                        println!("Echoing {} bytes: {:x?} ... {:x?}", value.len(), &value[0..4.min(value.len())], &value[value.len().saturating_sub(4) ..]);
                        if value.len() < 512 {
                            println!();
                        }
                        if let Err(err) = write_opt.as_mut().unwrap().write_all(&value).await {
                            println!("Write failed: {}", &err);
                            write_opt = None;
                        }
                    }
                    Err(err) => {
                        println!("Read stream error: {}", &err);
                        read_opt = None;
                    }
                }
            }

            evt = cpu_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {}", notifier.mtu());
                                                                            cpu_load_writer_opt = Some(notifier);
                    },
                    None => break,
                _ => {break}}
            },
            evt = temp_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {}", notifier.mtu());
                                                                            temp_writer_opt = Some(notifier);
                    },
                    None => break,
                _ => {break}}
            },
            evt = memory_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {}", notifier.mtu());
                                                                            memory_writer_opt = Some(notifier);
                    },
                    None => break,
                _ => {break}}
            }, evt = uptime_control.next() => {
                match evt {
                    Some(CharacteristicControlEvent::Notify(notifier)) => {
                        println!("Accepting notify request event with MTU {}", notifier.mtu());
                                                                            uptime_writer_opt = Some(notifier);
                    },
                    None => break,
                _ => {break}}
            },
            _ = time::sleep(Duration::from_secs(1)) => {
                let cpu_load = sys.cpu_load_aggregate()?.done()?;
                let system_cpu_load = cpu_load.system;
                let cpu_temperature = sys.cpu_temp()?;
                let memory_usage = sys.memory()?;
                let uptime = sys.uptime()?;
                let uptime_minutes = uptime.as_secs()/60;

                println!("CPU LOAD is: {system_cpu_load}");
                println!("CPU TEMP is: {cpu_temperature}");
                println!("Memory Usage is: {}/{}", memory_usage.total, memory_usage.free);

                if let Some(writer) = &mut cpu_load_writer_opt {
                    writer.write_f32(system_cpu_load).await?;
                    println!("Updated CPU load characteristic: {:.2}%", system_cpu_load);
                }
                if let Some(writer) = &mut temp_writer_opt {
                    writer.write_f32(cpu_temperature).await?;
                    println!("Updated CPU temp characteristic: {:.2}C", cpu_temperature);
                }
               if let Some(writer) = &mut memory_writer_opt {
                    let used_memory = memory_usage.total.as_u64() - memory_usage.free.as_u64();
                    let used_memory = used_memory as f64 / 1024f64/ 1024f64;
                    let total_memory = memory_usage.total.as_u64() as f64 / 1024f64 / 1024f64;
                    let usage = format!("{:.2}/{:.2} MB", used_memory, total_memory);
                    writer.write_all(&usage.clone().into_bytes()).await?;
                    writer.flush().await?;
                    println!("Updated Memory usage: {usage}");
                }
                if let Some(writer) = &mut uptime_writer_opt {
                    writer.write_u64(uptime_minutes).await?;
                    println!("Updated Uptime Minutes characteristic: {uptime_minutes}");
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
