use opentelemetry::InstrumentationScope;
use opentelemetry_sdk::logs::LogExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::{
    error::OTelSdkResult,
    logs::{LogBatch, SdkLogRecord},
};
use std::borrow::Cow;
use std::collections::HashSet;
use std::error::Error;

use crate::logs::exporter::{DefaultEventNameCallback, EventNameCallback, UserEventsExporter};

/// Processes and exports logs to user_events.
///
/// This processor exports logs without synchronization.
/// It is specifically designed for the user_events exporter, where
/// the underlying exporter is safe under concurrent calls.
pub struct Processor<C = DefaultEventNameCallback>
where
    C: EventNameCallback,
{
    exporter: UserEventsExporter<C>,
}

impl<C> std::fmt::Debug for Processor<C>
where
    C: EventNameCallback,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Processor")
            .field("exporter", &self.exporter)
            .finish()
    }
}

impl Processor<DefaultEventNameCallback> {
    /// Creates a builder for configuring a user_events Processor
    pub fn builder(provider_name: &str) -> ProcessorBuilder {
        ProcessorBuilder::new(provider_name)
    }
}

impl<C> opentelemetry_sdk::logs::LogProcessor for Processor<C>
where
    C: EventNameCallback,
{
    fn emit(&self, record: &mut SdkLogRecord, scope: &InstrumentationScope) {
        let log_tuple = &[(record as &SdkLogRecord, scope)];
        // TODO: Using futures_executor::block_on can make the code non reentrant safe
        // if that crate starts emitting logs that are bridged to OTel.
        // TODO: How to log if export() returns Err? Maybe a metric?
        // Alternately, we can enter a SuppressionContext and log the error
        // if the result is an error (once upstream ships SuppressionContext).
        let _ = futures_executor::block_on(self.exporter.export(LogBatch::new(log_tuple)));
    }

    // Nothing to flush as this processor does not buffer
    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown(&self) -> OTelSdkResult {
        self.exporter.shutdown()
    }

    #[cfg(feature = "spec_unstable_logs_enabled")]
    fn event_enabled(
        &self,
        level: opentelemetry::logs::Severity,
        target: &str,
        name: Option<&str>,
    ) -> bool {
        self.exporter.event_enabled(level, target, name)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.exporter.set_resource(resource);
    }
}

/// Builder for configuring and constructing a user_events Processor
pub struct ProcessorBuilder<'a, C = DefaultEventNameCallback>
where
    C: EventNameCallback,
{
    provider_name: &'a str,
    resource_attribute_keys: HashSet<Cow<'static, str>>,
    event_name_callback: C,
}

impl<'a, C> std::fmt::Debug for ProcessorBuilder<'a, C>
where
    C: EventNameCallback,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessorBuilder")
            .field("provider_name", &self.provider_name)
            .field("resource_attribute_keys", &self.resource_attribute_keys)
            .field("event_name_callback", &std::any::type_name::<C>())
            .finish()
    }
}

impl<'a> ProcessorBuilder<'a, DefaultEventNameCallback> {
    /// Creates a new builder with the given provider name
    ///
    /// The provider name must:
    /// - Not be empty.
    /// - Be less than 234 characters.
    /// - Contain only ASCII letters, digits, and the underscore (`'_'`) character.
    /// - Be short, human-readable, and unique enough to avoid conflicts with other provider names.
    /// - Typically include a company name and a component name, e.g., "MyCompany_MyComponent".    
    ///
    /// Tracepoint names are generated by combining the provider name, event
    /// level and keyword (currently hardcoded to `1`) in the following format:
    /// `ProviderName + '_' + 'L' + EventLevel + 'K' + EventKeyword`
    ///
    /// For example, if "myprovider" is the provider name, the following tracepoint names are created:
    /// - `myprovider_L5K1`
    /// - `myprovider_L4K1`
    /// - `myprovider_L3K1`
    /// - `myprovider_L2K1`
    /// - `myprovider_L1K1`
    ///
    /// perf tool can be used to record events from the tracepoints.
    /// For example the following will capture level 2 (Error) and 3(Warning) events:
    /// perf record -e user_events:myprovider_L2K1,user_events:myprovider_L3K1
    pub(crate) fn new(provider_name: &'a str) -> Self {
        Self {
            provider_name,
            resource_attribute_keys: HashSet::new(),
            event_name_callback: DefaultEventNameCallback,
        }
    }
}

impl<'a, C> ProcessorBuilder<'a, C>
where
    C: EventNameCallback,
{
    /// Sets the resource attributes for the processor.
    ///
    /// This specifies which resource attributes should be exported with each log record.
    ///
    /// # Performance Considerations
    ///
    /// **Warning**: Each specified resource attribute will be serialized and sent
    /// with EVERY log record. This is different from OTLP exporters where resource
    /// attributes are serialized once per batch. Consider the performance impact
    /// when selecting which attributes to export.
    ///
    /// # Best Practices for user_events
    ///
    /// **Recommendation**: Be selective about which resource attributes to export.
    /// Since user_events writes to a local kernel buffer and requires a local
    /// listener/agent, the agent can often deduce many resource attributes without
    /// requiring them to be sent with each log:
    ///
    /// - **Infrastructure attributes** (datacenter, region, availability zone) can
    ///   be determined by the local agent.
    /// - **Host attributes** (hostname, IP address, OS version) are available locally.
    /// - **Deployment attributes** (environment, cluster) may be known to the agent.
    ///
    /// Focus on attributes that are truly specific to your application instance
    /// and cannot be easily determined by the local agent.
    ///
    /// Nevertheless, if there are attributes that are fixed and must be emitted
    /// with every log, modeling them as Resource attributes and using this method
    /// is much more efficient than emitting them explicitly with every log.
    pub fn with_resource_attributes<I, S>(mut self, attributes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<Cow<'static, str>>,
    {
        self.resource_attribute_keys = attributes.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Sets a callback for determining event names
    #[cfg(feature = "experimental_eventname_callback")]
    pub fn with_event_name_callback<NewC>(self, callback: NewC) -> ProcessorBuilder<'a, NewC>
    where
        NewC: EventNameCallback,
    {
        ProcessorBuilder {
            provider_name: self.provider_name,
            resource_attribute_keys: self.resource_attribute_keys,
            event_name_callback: callback,
        }
    }

    /// Builds the processor with the configured callback
    pub fn build(self) -> Result<Processor<C>, Box<dyn Error>> {
        // Validate provider name
        if self.provider_name.is_empty() {
            return Err("Provider name cannot be empty.".into());
        }
        if self.provider_name.len() >= 234 {
            return Err("Provider name must be less than 234 characters.".into());
        }
        if !self
            .provider_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err("Provider name must contain only ASCII letters, digits, and '_'.".into());
        }

        let exporter = UserEventsExporter::new(
            self.provider_name,
            self.resource_attribute_keys,
            self.event_name_callback,
        );
        Ok(Processor { exporter })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::logs::Logger;
    use opentelemetry::logs::LoggerProvider;
    use opentelemetry_sdk::logs::{LogProcessor, SdkLoggerProvider};

    #[test]
    fn test_processor_builder_with_valid_provider() {
        let processor = Processor::builder("test_provider").build();
        assert!(processor.is_ok());
    }

    #[test]
    fn test_processor_builder_with_empty_provider_name() {
        let processor = Processor::builder("").build();
        assert!(processor.is_err());
        assert_eq!(
            processor.unwrap_err().to_string(),
            "Provider name cannot be empty."
        );
    }

    #[test]
    fn test_processor_builder_with_long_provider_name() {
        let long_name = "a".repeat(234);
        let processor = Processor::builder(&long_name).build();
        assert!(processor.is_err());
    }

    #[test]
    fn test_processor_builder_with_invalid_chars() {
        let invalid_name = "test-provider";
        let processor = Processor::builder(invalid_name).build();
        assert!(processor.is_err());
    }

    #[test]
    fn valid_provider_name() {
        let valid_names = vec![
            "ValidName",
            "valid_name",
            "Valid123",
            "valid_123",
            "_valid_name",
            "VALID_NAME",
        ];

        for valid_name in valid_names {
            let processor = Processor::builder(valid_name).build();
            assert!(processor.is_ok());
        }
    }

    #[test]
    fn provider_name_contains_invalid_characters() {
        // Define a vector of invalid provider names to test
        let invalid_names = vec![
            "Invalid Name",  // space
            "Invalid:Name",  // colon
            "Invalid\0Name", // null character
            "Invalid-Name",  // hyphen
            "InvalidName!",  // exclamation mark
            "InvalidName@",  // at symbol
            "Invalid+Name",  // plus
            "Invalid&Name",  // ampersand
            "Invalid#Name",  // hash
            "Invalid%Name",  // percent
            "Invalid/Name",  // slash
            "Invalid\\Name", // backslash
            "Invalid=Name",  // equals
            "Invalid?Name",  // question mark
            "Invalid;Name",  // semicolon
            "Invalid,Name",  // comma
        ];

        // Expected error message
        let expected_error = "Provider name must contain only ASCII letters, digits, and '_'.";

        // Test each invalid name
        for invalid_name in invalid_names {
            let options = Processor::builder(invalid_name).build();
            // Assert that the result is an error
            assert!(
                options.is_err(),
                "Expected '{invalid_name}' to be invalid, but it was accepted"
            );

            // Assert that the error message is as expected
            assert_eq!(
                options.err().unwrap().to_string(),
                expected_error,
                "Wrong error message for invalid name: '{invalid_name}'"
            );
        }
    }

    #[test]
    fn test_shutdown() {
        let processor = Processor::builder("test_provider").build().unwrap();
        assert!(processor.shutdown().is_ok());
    }

    #[test]
    fn test_force_flush() {
        let processor = Processor::builder("test_provider").build().unwrap();
        assert!(processor.force_flush().is_ok());
    }

    #[test]
    fn test_emit() {
        let processor = Processor::builder("test_provider").build().unwrap();

        let mut record = SdkLoggerProvider::builder()
            .build()
            .logger("test")
            .create_log_record();
        let instrumentation = Default::default();
        // No assertions here, simply calling the function to ensure it doesn't panic
        processor.emit(&mut record, &instrumentation);
    }

    #[test]
    #[cfg(feature = "spec_unstable_logs_enabled")]
    fn test_event_enabled() {
        let processor = Processor::builder("test_provider").build().unwrap();

        // No assertions here, simply calling the function to ensure it doesn't panic
        let _info_enabled =
            processor.event_enabled(opentelemetry::logs::Severity::Info, "test", Some("test"));
        let _debug_enabled =
            processor.event_enabled(opentelemetry::logs::Severity::Debug, "test", Some("test"));
        let _error_enabled =
            processor.event_enabled(opentelemetry::logs::Severity::Error, "test", Some("test"));

        // Test completes if no panics occur
    }
}
