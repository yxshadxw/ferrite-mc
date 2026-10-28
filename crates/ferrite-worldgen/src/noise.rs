//! `worldgen/noise` noise parameters.

use serde::{Deserialize, Serialize};

/// Vanilla noise parameters (the 26.3 shape, not the older `firstOctave`/`amplitudes` one).
///
/// `octave_count` and `amplitude_modifiers` are optional in the data: most vanilla noises
/// leave them out and use the default one octave with no amplitude modifiers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseParameters {
    pub base_octave: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub octave_count: Option<u32>,
    pub base_amplitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amplitude_modifiers: Option<Vec<f64>>,
}

impl NoiseParameters {
    /// The number of octaves the sampler runs: `octave_count`, or vanilla's implicit one.
    pub fn octaves(&self) -> u32 {
        self.octave_count.unwrap_or(1)
    }

    /// The amplitude modifier per octave, or vanilla's implicit `[1.0]`.
    pub fn amplitudes(&self) -> &[f64] {
        self.amplitude_modifiers.as_deref().unwrap_or(&[1.0])
    }
}

#[cfg(test)]
mod tests {
    use super::NoiseParameters;

    #[test]
    fn round_trips() {
        let json = r#"{
            "amplitude_modifiers": [1.0, 1.0, 2.0],
            "base_amplitude": 0.8880832896205223,
            "base_octave": -9,
            "octave_count": 3
        }"#;
        let noise: NoiseParameters = serde_json::from_str(json).unwrap();
        assert_eq!(noise.octaves(), 3);
        assert_eq!(noise.amplitudes(), [1.0, 1.0, 2.0]);
        let back: NoiseParameters =
            serde_json::from_str(&serde_json::to_string(&noise).unwrap()).unwrap();
        assert_eq!(back, noise);
    }

    #[test]
    fn omitted_fields_use_vanilla_defaults() {
        let json = r#"{"base_amplitude": 1.0, "base_octave": -5}"#;
        let noise: NoiseParameters = serde_json::from_str(json).unwrap();
        assert_eq!(noise.octaves(), 1);
        assert_eq!(noise.amplitudes(), [1.0]);
    }
}
