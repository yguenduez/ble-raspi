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
use futures::{pin_mut, StreamExt};
use std::str::FromStr;
use std::time::Duration;
use tokio::{io::AsyncWriteExt, time, time::sleep};

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

    pin_mut!(cpu_control);
    pin_mut!(temp_control);
    pin_mut!(memory_control);
    pin_mut!(uptime_control);

    let sys = System::new();

    loop {
        tokio::select! {
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
                    let total_memory = memory_usage.total.as_u64();
                    let usage = format!("{used_memory}/{total_memory} MB");
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
