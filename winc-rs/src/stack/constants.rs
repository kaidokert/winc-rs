// Copyright 2025 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Re-export commonly used constants from manager layer for convenience
pub use crate::manager::constants::MAX_HOST_NAME_LEN;

/// Maximum length of data that can be sent in a single send operation.
///
/// This limit is imposed by the WINC1500 hardware/firmware and applies
/// to both TCP and UDP send operations.
pub const MAX_SEND_LENGTH: usize = 1400;

/// Maximum length for testing purposes.
///
/// Used in unit tests to verify buffer size validation logic.
#[cfg(test)]
pub const MAX_SEND_LENGTH_TEST: usize = 4;
