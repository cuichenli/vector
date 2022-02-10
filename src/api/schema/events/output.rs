use async_graphql::Union;

use super::{
    log::Log,
    metric::Metric,
    notification::{EventNotification, EventNotificationType},
    trace::Trace,
};
use crate::api::tap::{TapNotification, TapPayload};

#[derive(Union, Debug, Clone)]
/// An event or a notification
pub enum OutputEventsPayload {
    /// Log event
    Log(Log),

    /// Metric event
    Metric(Metric),

    // Notification
    Notification(EventNotification),

    /// Trace event
    Trace(Trace),
}

/// Convert an `api::TapPayload` to the equivalent GraphQL type.
impl From<TapPayload> for Vec<OutputEventsPayload> {
    fn from(t: TapPayload) -> Self {
        match t {
            TapPayload::Log(output, log_array) => log_array
                .into_iter()
                .map(|log| OutputEventsPayload::Log(Log::new(output.clone(), log)))
                .collect(),
            TapPayload::Metric(output, metric_array) => metric_array
                .into_iter()
                .map(|metric| OutputEventsPayload::Metric(Metric::new(output.clone(), metric)))
                .collect(),
            TapPayload::Notification(component_key, n) => match n {
                TapNotification::Matched => vec![OutputEventsPayload::Notification(
                    EventNotification::new(component_key, EventNotificationType::Matched),
                )],
                TapNotification::NotMatched => vec![OutputEventsPayload::Notification(
                    EventNotification::new(component_key, EventNotificationType::NotMatched),
                )],
            },
            TapPayload::Trace(output, trace_array) => trace_array
                .into_iter()
                .map(|trace| OutputEventsPayload::Trace(Trace::new(output.clone(), trace)))
                .collect(),
        }
    }
}
