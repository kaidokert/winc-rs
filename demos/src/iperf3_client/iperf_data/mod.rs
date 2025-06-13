use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serializer};
use serde_json_core::heapless;

// Custom serializer/deserializer for retransmit fields
// Deserializes any value to 0 (ignores what server sends)
// Serializes as 0 (consistent value for server)
fn deserialize_ignore_retransmits<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    // Ignore whatever value is sent and always return 0
    let _ = IgnoredAny::deserialize(deserializer)?;
    Ok(0)
}

fn serialize_retransmits<S>(_value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Always serialize as 0, ignore the actual value
    serializer.serialize_u64(0)
}

#[derive(Debug, Clone)]
#[allow(unused)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Cmds {
    TestStart = 1,
    TestRunning = 2,
    TestEnd = 4,
    ParamExchange = 9,
    CreateStreams = 10,
    ServerTerminate = 11,
    ClientTerminate = 12,
    ExchangeResults = 13,
    DisplayResults = 14,
    IperfStart = 15,
    IperfDone = 16,
    AccessDenied = -1,
    ServerError = -2,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct SessionConfig {
    pub tcp: u8,
    pub num: usize,
    pub len: usize,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct UdpSessionConfig {
    pub udp: bool,
    pub omit: u32,
    pub time: u32,
    pub num: usize,
    pub blockcount: u32,
    pub parallel: u32,
    pub len: usize,
    pub bandwidth: u32,
    pub pacing_timer: u32,
    pub client_version: heapless::String<16>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct UdpPacketHeader {
    pub tv_sec: u32,
    pub tv_usec: u32,
    pub id: u32,
}

impl UdpPacketHeader {
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&self.tv_sec.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.tv_usec.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.id.to_be_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 12 {
            return None;
        }
        Some(Self {
            tv_sec: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            tv_usec: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            id: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        })
    }
}

#[derive(Debug, Default)]
pub struct UdpMetrics {
    pub packets_sent: u32,
    pub bytes_sent: u32,
    pub jitter_sum: f32,
    pub jitter_count: u32,
    pub errors: u32,
}

impl UdpMetrics {
    pub fn calculate_jitter(&self) -> f32 {
        if self.jitter_count > 0 {
            self.jitter_sum / self.jitter_count as f32
        } else {
            0.0
        }
    }
}
impl SessionConfig {
    const MAX_SESSION_CONF_LEN: usize = 80;
    pub fn serde_json(
        &self,
    ) -> Result<heapless::String<{ Self::MAX_SESSION_CONF_LEN }>, serde_json_core::ser::Error> {
        serde_json_core::to_string(self)
    }
}

impl UdpSessionConfig {
    const MAX_UDP_SESSION_CONF_LEN: usize = 200;
    pub fn serde_json(
        &self,
    ) -> Result<heapless::String<{ Self::MAX_UDP_SESSION_CONF_LEN }>, serde_json_core::ser::Error>
    {
        serde_json_core::to_string(self)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct StreamResults {
    pub id: u8,
    pub bytes: u32,
    #[serde(
        default,
        deserialize_with = "deserialize_ignore_retransmits",
        serialize_with = "serialize_retransmits"
    )]
    pub retransmits: u64, // Always include retransmits field for server compatibility
    pub jitter: f32,
    pub errors: u32,
    pub packets: u32,
    pub start_time: f32,
    pub end_time: f32,
}

impl StreamResults {
    const MAX_STREAM_RESULTS_LEN: usize = 200;
    #[allow(unused)]
    pub fn serde_json(
        &self,
    ) -> Result<heapless::String<{ Self::MAX_STREAM_RESULTS_LEN }>, serde_json_core::ser::Error>
    {
        serde_json_core::to_string(self)
    }
}

pub const MAX_SESSION_RESULTS_LEN: usize = StreamResults::MAX_STREAM_RESULTS_LEN + 100;

#[derive(serde::Serialize, serde::Deserialize, Default, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SessionResults<const N: usize> {
    pub cpu_util_total: f32,
    pub cpu_util_user: f32,
    pub cpu_util_system: f32,
    #[serde(
        default,
        deserialize_with = "deserialize_ignore_retransmits",
        serialize_with = "serialize_retransmits"
    )]
    pub sender_has_retransmits: u64, // Always include for server compatibility
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub congestion_used: Option<heapless::String<16>>, // Optional field from remote servers
    pub streams: heapless::Vec<StreamResults, N>,
}
impl<const N: usize> SessionResults<N> {
    pub fn serde_json(
        &self,
    ) -> Result<heapless::String<{ MAX_SESSION_RESULTS_LEN }>, serde_json_core::ser::Error> {
        serde_json_core::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use core::u8;

    use super::*;

    const MAX_CONF_LEN: usize = 80;

    #[test]
    fn session_conf_serialize() {
        let conf = SessionConfig {
            tcp: 1,
            num: 32,
            len: 32,
        };
        let j = conf.serde_json().unwrap();
        assert_eq!(j, "{\"tcp\":1,\"num\":32,\"len\":32}");
        let conf = SessionConfig {
            tcp: u8::MAX,
            num: usize::MAX,
            len: usize::MAX,
        };
        let j = serde_json_core::to_string::<_, MAX_CONF_LEN>(&conf).unwrap();
        assert_eq!(
            j,
            "{\"tcp\":255,\"num\":18446744073709551615,\"len\":18446744073709551615}"
        );
    }

    #[test]
    fn stream_result_serialize() {
        let results = StreamResults {
            id: 1,
            ..Default::default()
        };
        let j = results.serde_json().unwrap();
        assert_eq!(
            j,
            concat!(
                r#"{"id":1,"#,
                r#""bytes":0,"#,
                r#""retransmits":0,"#,
                r#""jitter":0.0,"#,
                r#""errors":0,"#,
                r#""packets":0,"#,
                r#""start_time":0.0,"#,
                r#""end_time":0.0}"#
            )
        );
        let j = StreamResults {
            id: u8::MAX,
            bytes: u32::MAX,
            retransmits: u64::MAX,
            jitter: f32::MAX,
            errors: u32::MAX,
            packets: u32::MAX,
            start_time: 10000.0,
            end_time: 10000.0,
        }
        .serde_json()
        .unwrap();
        assert_eq!(
            j,
            concat!(
                r#"{"id":255,"#,
                r#""bytes":4294967295,"#,
                r#""retransmits":0,"#,
                r#""jitter":3.4028235e38,"#,
                r#""errors":4294967295,"#,
                r#""packets":4294967295,"#,
                r#""start_time":10000.0,"#,
                r#""end_time":10000.0}"#
            )
        );
    }

    #[test]
    fn session_results_serialize() {
        let results = SessionResults::<1> {
            streams: heapless::Vec::from_slice(&[StreamResults {
                id: 1,
                bytes: u32::MAX,
                retransmits: u64::MAX,
                jitter: f32::MAX,
                errors: u32::MAX,
                packets: u32::MAX,
                start_time: 10000.0,
                end_time: 10000.0,
            }])
            .unwrap(),
            cpu_util_system: 1000.0,
            cpu_util_user: 1000.0,
            cpu_util_total: 1000.0,
            sender_has_retransmits: u64::MAX,
            congestion_used: None,
        };
        let j = results.serde_json().unwrap();
        assert_eq!(
            j,
            concat!(
                r#"{"cpu_util_total":1000.0,"#,
                r#""cpu_util_user":1000.0,"#,
                r#""cpu_util_system":1000.0,"#,
                r#""sender_has_retransmits":0,"#,
                r#""streams":[{"#,
                r#""id":1,"#,
                r#""bytes":4294967295,"#,
                r#""retransmits":0,"#,
                r#""jitter":3.4028235e38,"#,
                r#""errors":4294967295,"#,
                r#""packets":4294967295,"#,
                r#""start_time":10000.0,"#,
                r#""end_time":10000.0}]}"#
            )
        );
    }

    #[test]
    fn parse_remote_server_udp_response() {
        // Real response from remote server (34.19.56.238) - exact format as received
        let server_json = r#"{"cpu_util_total":10.359712832687853,"cpu_util_user":1.8660955352355173,"cpu_util_system":8.4936172974523352,"sender_has_retransmits":18446744073709551615,"streams":[{"id":1,"bytes":32410112,"retransmits":18446744073709551615,"jitter":4.2784303898839865e-05,"errors":914275,"packets":977576,"start_time":0,"end_time":3.821175}]}"#;

        let result: Result<(SessionResults<1>, usize), _> = serde_json_core::from_str(server_json);
        match result {
            Ok((session_results, _)) => {
                assert_eq!(session_results.streams[0].bytes, 32410112);
                assert_eq!(session_results.streams[0].packets, 977576);
                assert_eq!(session_results.streams[0].errors, 914275);
                assert!(session_results.streams[0].end_time > 0.0);
                // Retransmit values are ignored during deserialization and always become 0
                assert_eq!(session_results.streams[0].retransmits, 0);
                assert_eq!(session_results.sender_has_retransmits, 0);
            }
            Err(e) => {
                panic!("Failed to parse remote server UDP response: {:?}", e);
            }
        }
    }

    #[test]
    fn parse_remote_server_udp_response_without_large_numbers() {
        // Same response but with large numbers removed to test if that's the issue
        let server_json = r#"{"cpu_util_total":10.359712832687853,"cpu_util_user":1.8660955352355173,"cpu_util_system":8.4936172974523352,"streams":[{"id":1,"bytes":32410112,"jitter":4.2784303898839865e-05,"errors":914275,"packets":977576,"start_time":0,"end_time":3.821175}]}"#;

        let result: Result<(SessionResults<1>, usize), _> = serde_json_core::from_str(server_json);
        match result {
            Ok((session_results, _)) => {
                assert_eq!(session_results.streams[0].bytes, 32410112);
                assert_eq!(session_results.streams[0].packets, 977576);
                assert_eq!(session_results.streams[0].errors, 914275);
                assert!(session_results.streams[0].end_time > 0.0);
                // These should be 0 since they were omitted and we use default values
                assert_eq!(session_results.streams[0].retransmits, 0);
                assert_eq!(session_results.sender_has_retransmits, 0);
            }
            Err(e) => {
                panic!(
                    "Failed to parse remote server UDP response without large numbers: {:?}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_large_number_parsing() {
        // Test if the large number itself is the issue
        #[derive(serde::Deserialize)]
        struct TestStruct {
            a: u64,
        }

        let test_json = r#"{"a":18446744073709551615}"#;
        let result: Result<(TestStruct, usize), _> = serde_json_core::from_str(test_json);
        match result {
            Ok(_) => {
                // If this passes, large numbers aren't the issue
            }
            Err(e) => {
                panic!("Failed to parse large number: {:?}", e);
            }
        }
    }

    #[test]
    fn test_scientific_notation() {
        // Test if scientific notation is the issue
        #[derive(serde::Deserialize)]
        struct TestStruct {
            jitter: f32,
        }

        let test_json = r#"{"jitter":4.2784303898839865e-05}"#;
        let result: Result<(TestStruct, usize), _> = serde_json_core::from_str(test_json);
        match result {
            Ok(_) => {
                // If this passes, scientific notation isn't the issue
            }
            Err(e) => {
                panic!("Failed to parse scientific notation: {:?}", e);
            }
        }
    }

    #[test]
    fn test_minimal_session_results() {
        // Test with minimal SessionResults that should work
        let test_json = r#"{"cpu_util_total":1.0,"cpu_util_user":1.0,"cpu_util_system":1.0,"streams":[{"id":1,"bytes":1000,"jitter":0,"errors":0,"packets":10,"start_time":0,"end_time":1.0}]}"#;
        let result: Result<(SessionResults<1>, usize), _> = serde_json_core::from_str(test_json);
        match result {
            Ok(_) => {
                // If this passes, our SessionResults struct is OK
            }
            Err(e) => {
                panic!("Failed to parse minimal SessionResults: {:?}", e);
            }
        }
    }

    #[test]
    fn parse_remote_server_tcp_response() {
        // Real response from remote server (34.19.56.238) with negative retransmits and congestion_used
        let server_json = r#"{
            "cpu_util_total": 100.0019774569903,
            "cpu_util_user": 24.243622701206249,
            "cpu_util_system": 75.756377298793751,
            "sender_has_retransmits": 18446744073709551615,
            "congestion_used": "cubic",
            "streams": [
                {
                    "id": 1,
                    "bytes": 9728,
                    "retransmits": 18446744073709551615,
                    "jitter": 0,
                    "errors": 0,
                    "packets": 0,
                    "start_time": 0,
                    "end_time": 0.031273
                }
            ]
        }"#;

        let result: Result<(SessionResults<1>, usize), _> = serde_json_core::from_str(server_json);
        match result {
            Ok((session_results, _)) => {
                assert_eq!(session_results.streams[0].bytes, 9728);
                assert_eq!(session_results.streams[0].packets, 0);
                assert_eq!(session_results.streams[0].errors, 0);
                assert!(session_results.streams[0].end_time > 0.0);
                assert_eq!(session_results.congestion_used.as_ref().unwrap(), "cubic");
            }
            Err(e) => {
                panic!("Failed to parse remote server TCP response: {:?}", e);
            }
        }
    }

    #[test]
    fn test_udp_client() {
        let server_json = r#"{"cpu_util_total":0.14083125251201814,
            "cpu_util_user":0.024052079080704222,
            "cpu_util_system":0.119943920678775,
            "sender_has_retransmits":-1,
            "streams":[{"id":1,"bytes":256,"retransmits":-1,"jitter":0,"errors":0,"omitted_errors":0,"packets":0,"omitted_packets":0,"start_time":0,"end_time":0.315936}]}"#;
        let result: Result<(SessionResults<1>, usize), _> = serde_json_core::from_str(server_json);
        match result {
            Ok((session_results, _)) => {
                assert_eq!(session_results.streams[0].bytes, 256);
                // Retransmit values are ignored during deserialization and always become 0
                assert_eq!(session_results.streams[0].retransmits, 0);
                assert_eq!(session_results.sender_has_retransmits, 0);
            }
            Err(e) => {
                panic!("Failed to parse UDP client response: {:?}", e);
            }
        }
    }

    #[test]
    fn test_what_we_send_to_server() {
        // Test what our client actually sends to see if it's compatible with server expectations

        // TCP client results - minimal like original main branch
        let tcp_results = SessionResults::<1> {
            streams: heapless::Vec::from_slice(&[StreamResults {
                id: 1,
                bytes: 1024000,
                ..Default::default()
            }])
            .unwrap(),
            ..Default::default()
        };
        let tcp_json = tcp_results.serde_json().unwrap();

        // Compare with the format from our serialization test - current format includes:
        // {"cpu_util_total":0.0,"cpu_util_user":0.0,"cpu_util_system":0.0,"streams":[{"id":1,"bytes":1024000,"jitter":0.0,"errors":0,"packets":0,"start_time":0.0,"end_time":0.0}]}
        // But original main branch had jitter:u32 not f32, and retransmits:u64 not Option<u64>

        // The key difference: original sent "jitter":0 (u32), we now send "jitter":0.0 (f32)
        // Original sent "retransmits":0 always, we now skip it when None
        assert!(tcp_json.contains("\"bytes\":1024000"));
        assert!(tcp_json.contains("\"jitter\":0")); // Check if we're sending jitter as expected

        // The problem might be that retransmits field is missing when it's None
        // Let's check if our current JSON is missing retransmits
        if !tcp_json.contains("\"retransmits\":") {
            // This could be the problem - server might expect retransmits field always
            panic!("Missing retransmits field in JSON: {}", tcp_json);
        }
    }
}
