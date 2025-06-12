use serde_json_core::heapless;

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
    pub packets_received: u32,
    pub bytes_sent: u32,
    pub bytes_received: u32,
    pub jitter_sum: f32,
    pub jitter_count: u32,
    pub last_transit_time: f32,
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

    pub fn packet_loss_percent(&self) -> f32 {
        if self.packets_sent > 0 {
            ((self.packets_sent - self.packets_received) as f32 / self.packets_sent as f32) * 100.0
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

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct StreamResults {
    pub id: u8,
    pub bytes: u32,
    pub retransmits: u64, // Handle large unsigned values like 18446744073709551615
    pub jitter: u32,
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
    pub sender_has_retransmits: u64, // Handle large unsigned values like 18446744073709551615
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
                r#""jitter":0,"#,
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
            jitter: u32::MAX,
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
                r#""retransmits":18446744073709551615,"#,
                r#""jitter":4294967295,"#,
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
                jitter: u32::MAX,
                errors: u32::MAX,
                packets: u32::MAX,
                start_time: 10000.0,
                end_time: 10000.0,
            }])
            .unwrap(),
            cpu_util_system: 1000.0,
            cpu_util_user: 1000.0,
            cpu_util_total: 1000.0,
            sender_has_retransmits: 1000,
        };
        let j = results.serde_json().unwrap();
        assert_eq!(
            j,
            concat!(
                r#"{"cpu_util_total":1000.0,"#,
                r#""cpu_util_user":1000.0,"#,
                r#""cpu_util_system":1000.0,"#,
                r#""sender_has_retransmits":1000,"#,
                r#""streams":[{"#,
                r#""id":1,"#,
                r#""bytes":4294967295,"#,
                r#""retransmits":18446744073709551615,"#,
                r#""jitter":4294967295,"#,
                r#""errors":4294967295,"#,
                r#""packets":4294967295,"#,
                r#""start_time":10000.0,"#,
                r#""end_time":10000.0}]}"#
            )
        );
    }
}
