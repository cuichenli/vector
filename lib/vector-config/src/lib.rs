use std::{time::Duration, marker::PhantomData};

use schemars::{JsonSchema, schema::{SchemaObject, InstanceType, SingleOrVec, NumberValidation, Schema}, gen::SchemaGenerator};
use serde::{Deserialize, Serialize, Serializer, Deserializer};
use serde_with::{SerializeAs, DeserializeAs, DurationSeconds, formats::Strict};

#[derive(Copy, Clone, Debug, Default)]
pub struct AsSchema<T: ?Sized>(PhantomData<T>);

impl<T: JsonSchema + ?Sized> AsSchema<T> {
    pub fn serialize<S, I>(value: &I, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: SerializeAs<I>,
        I: ?Sized,
    {
        T::serialize_as(value, serializer)
    }

    pub fn deserialize<'de, D, I>(deserializer: D) -> Result<I, D::Error>
    where
        T: DeserializeAs<'de, I>,
        D: Deserializer<'de>,
    {
        T::deserialize_as(deserializer)
    }
}

impl<T: JsonSchema> JsonSchema for AsSchema<T> {
    fn schema_name() -> String {
        <T as JsonSchema>::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> Schema {
        <T as JsonSchema>::json_schema(gen)
    }
}

struct DurationInSeconds;

impl SerializeAs<Duration> for DurationInSeconds {
    fn serialize_as<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {  
        DurationSeconds::<u64, Strict>::serialize_as(value, serializer)
    }
}

impl<'de> DeserializeAs<'de, Duration> for DurationInSeconds {
    fn deserialize_as<D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {  
        DurationSeconds::<u64, Strict>::deserialize_as(deserializer)
    }
}

impl JsonSchema for DurationInSeconds {
    fn schema_name() -> String {
        String::from("duration")
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
			instance_type: Some(SingleOrVec::from(InstanceType::Number)),
			number: Some(Box::new(NumberValidation { 
				minimum: Some(1.0),
				..Default::default()
			})),
			..Default::default()
		})
    }
}

/// Controls batching behavior.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct BatchSettings {
	#[schemars(range(min = 1))]
	pub max_events: Option<u32>,
	#[schemars(range(min = 1))]
	pub max_bytes: Option<u32>,
	#[serde(alias = "foo", with = "AsSchema::<DurationInSeconds>")]
	#[schemars(range(min = 1))]
    #[deprecated]
	pub max_timeout: Duration,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct BasicSinkConfig {
	/// The API endpoint to send requests to.
	pub api_endpoint: String,
	pub batch: BatchSettings,
	/// How often to reload the API key from the configuration service.
	#[serde(default = "default_api_key_reload_interval")]
	#[serde(with = "AsSchema::<DurationInSeconds>")]
	pub api_key_reload_interval: Duration,
}

const fn default_api_key_reload_interval() -> Duration {
	Duration::from_secs(30)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use schemars::{schema_for, schema::{Schema, SchemaObject, NumberValidation, ObjectValidation}};

    use crate::BasicSinkConfig;

	#[test]
	fn output() {
		let mut schema = schema_for!(BasicSinkConfig);
        if let Some(obj) = &mut schema.schema.object {
            if let Some(Schema::Object(batch_prop_schema)) = obj.properties.get_mut("batch") {
                let mut batch_max_bytes_override_num = NumberValidation::default();
                batch_max_bytes_override_num.minimum = Some(5.0);

                let mut batch_max_bytes_override = SchemaObject::default();
                batch_max_bytes_override.number = Some(Box::new(batch_max_bytes_override_num));

                let mut batch_override = ObjectValidation::default();
                batch_override.properties.insert("max_bytes".to_string(), Schema::Object(batch_max_bytes_override));

                batch_prop_schema.object = Some(Box::new(batch_override));
            }
        }
		println!("{}", serde_json::to_string_pretty(&schema).unwrap());
	}
}
