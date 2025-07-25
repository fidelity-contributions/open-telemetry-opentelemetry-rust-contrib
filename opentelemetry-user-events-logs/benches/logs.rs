/*
    The benchmark results:
    criterion = "0.5.1"

    Hardware: Apple M4 Pro
    Total Number of Cores:	10
    (Inside multipass vm running Ubuntu 22.04)
    // When no listener
    | Test                        | Average time|
    |-----------------------------|-------------|
    | User_Event_4_Attributes     | 8 ns        |
    | User_Event_6_Attributes     | 8 ns        |

    // When listener is enabled
    // Run below to enable
    //  echo 1 | sudo tee /sys/kernel/debug/tracing/events/user_events/myprovider_L2K1/enable
    // Run below to disable
    //  echo 0 | sudo tee /sys/kernel/debug/tracing/events/user_events/myprovider_L2K1/enable
    | Test                        | Average time|
    |-----------------------------|-------------|
    | User_Event_4_Attributes     | 530 ns      |
    | User_Event_6_Attributes     | 586 ns      |
*/

// running the following from the current directory
// sudo -E ~/.cargo/bin/cargo bench --bench logs --all-features

use criterion::{criterion_group, criterion_main, Criterion};
use opentelemetry_appender_tracing::layer as tracing_layer;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::Resource;
#[cfg(feature = "experimental_eventname_callback")]
use opentelemetry_user_events_logs::EventNameCallback;
use opentelemetry_user_events_logs::Processor;
use tracing::error;
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

#[cfg(feature = "experimental_eventname_callback")]
struct EventNameFromLogRecordEventName;

#[cfg(feature = "experimental_eventname_callback")]
impl EventNameCallback for EventNameFromLogRecordEventName {
    #[inline(always)]
    fn get_name(&self, record: &opentelemetry_sdk::logs::SdkLogRecord) -> &'static str {
        record.event_name().unwrap_or("Log")
    }
}

#[cfg(feature = "experimental_eventname_callback")]
struct EventNameFromLogRecordCustom;

#[cfg(feature = "experimental_eventname_callback")]
impl EventNameCallback for EventNameFromLogRecordCustom {
    #[inline(always)]
    fn get_name(&self, record: &opentelemetry_sdk::logs::SdkLogRecord) -> &'static str {
        match record.event_name() {
            Some(name) if name.starts_with("Checkout") => "CheckoutEvent",
            Some(name) if name.starts_with("Payment") => "PaymentEvent",
            Some(_) => "OtherEvent",
            None => "DefaultEvent",
        }
    }
}

fn setup_provider_default() -> SdkLoggerProvider {
    let user_event_processor = Processor::builder("myprovider").build().unwrap();

    SdkLoggerProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_service_name("benchmark")
                .build(),
        )
        .with_log_processor(user_event_processor)
        .build()
}

#[cfg(feature = "experimental_eventname_callback")]
fn setup_provider_with_callback<C>(event_name_callback: C) -> SdkLoggerProvider
where
    C: EventNameCallback + 'static,
{
    let user_event_processor = Processor::builder("myprovider")
        .with_event_name_callback(event_name_callback)
        .build()
        .unwrap();

    SdkLoggerProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_service_name("benchmark")
                .build(),
        )
        .with_log_processor(user_event_processor)
        .build()
}

fn benchmark_4_attributes(c: &mut Criterion) {
    let provider = setup_provider_default();
    let ot_layer = tracing_layer::OpenTelemetryTracingBridge::new(&provider);
    let subscriber = Registry::default().with(ot_layer);

    tracing::subscriber::with_default(subscriber, || {
        c.bench_function("User_Event_4_Attributes", |b| {
            b.iter(|| {
                error!(
                    name : "CheckoutFailed",
                    field1 = "field1",
                    field2 = "field2",
                    field3 = "field3",
                    field4 = "field4",
                    message = "Unable to process checkout."
                );
            });
        });
    });
}

#[cfg(feature = "experimental_eventname_callback")]
fn benchmark_4_attributes_event_name_custom(c: &mut Criterion) {
    let provider = setup_provider_with_callback(EventNameFromLogRecordCustom);
    let ot_layer = tracing_layer::OpenTelemetryTracingBridge::new(&provider);
    let subscriber = Registry::default().with(ot_layer);

    tracing::subscriber::with_default(subscriber, || {
        c.bench_function("User_Event_4_Attributes_EventName_Custom", |b| {
            b.iter(|| {
                error!(
                    name : "CheckoutFailed",
                    field1 = "field1",
                    field2 = "field2",
                    field3 = "field3",
                    field4 = "field4",
                    message = "Unable to process checkout."
                );
            });
        });
    });
}

#[cfg(feature = "experimental_eventname_callback")]
fn benchmark_4_attributes_event_name_from_log_record(c: &mut Criterion) {
    let provider = setup_provider_with_callback(EventNameFromLogRecordEventName);
    let ot_layer = tracing_layer::OpenTelemetryTracingBridge::new(&provider);
    let subscriber = Registry::default().with(ot_layer);

    tracing::subscriber::with_default(subscriber, || {
        c.bench_function("User_Event_4_Attributes_EventName_FromLogRecord", |b| {
            b.iter(|| {
                error!(
                    name : "CheckoutFailed",
                    field1 = "field1",
                    field2 = "field2",
                    field3 = "field3",
                    field4 = "field4",
                    message = "Unable to process checkout."
                );
            });
        });
    });
}

fn benchmark_6_attributes(c: &mut Criterion) {
    let provider = setup_provider_default();
    let ot_layer = tracing_layer::OpenTelemetryTracingBridge::new(&provider);
    let subscriber = Registry::default().with(ot_layer);

    tracing::subscriber::with_default(subscriber, || {
        c.bench_function("User_Event_6_Attributes", |b| {
            b.iter(|| {
                error!(
                    name : "CheckoutFailed",
                    field1 = "field1",
                    field2 = "field2",
                    field3 = "field3",
                    field4 = "field4",
                    field5 = "field5",
                    field6 = "field6",
                    message = "Unable to process checkout."
                );
            });
        });
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    benchmark_4_attributes(c);
    benchmark_6_attributes(c);
    #[cfg(feature = "experimental_eventname_callback")]
    benchmark_4_attributes_event_name_custom(c);
    #[cfg(feature = "experimental_eventname_callback")]
    benchmark_4_attributes_event_name_from_log_record(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = criterion_benchmark
}
criterion_main!(benches);
