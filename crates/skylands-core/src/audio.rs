use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use skylands_formats::AudioClip;
use skylands_portal::protocol::{encode_audio_packet, REPORT_SIZE};
use std::collections::VecDeque;

pub fn resample_to_8khz_u8(clip: &AudioClip) -> Vec<u8> {
    if clip.samples.is_empty() {
        return Vec::new();
    }
    let channels = clip.channels.max(1) as usize;
    let frames = clip.samples.len() / channels;
    if frames == 0 {
        return Vec::new();
    }

    let src_rate = clip.sample_rate.max(1) as f64;
    let dst_rate = 8000.0f64;
    let out_len = ((frames as f64) * (dst_rate / src_rate)).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_idx = (i as f64 * src_rate / dst_rate).round() as usize;
        let frame_idx = src_idx.min(frames - 1);
        let s = if channels == 1 {
            clip.samples[frame_idx] as i32
        } else {
            (clip.samples[frame_idx * channels] as i32
                + clip.samples[frame_idx * channels + 1] as i32)
                / 2
        };
        let u = (((s + 32768) >> 8).clamp(0, 255)) as u8;
        out.push(u);
    }

    out
}

pub struct PortalSpeakerSink {
    seq: u8,
    queue: VecDeque<[u8; REPORT_SIZE]>,
}

impl PortalSpeakerSink {
    pub const SAMPLE_RATE: u32 = 8000;
    pub const CHUNK_SIZE: usize = 30;

    pub fn new() -> Self {
        Self {
            seq: 0,
            queue: VecDeque::new(),
        }
    }

    pub fn queue_pcm_samples(&mut self, samples: &[u8]) {
        for chunk in samples.chunks(Self::CHUNK_SIZE) {
            let pkt = encode_audio_packet(self.seq, chunk);
            self.seq = self.seq.wrapping_add(1);
            self.queue.push_back(pkt);
        }
    }

    pub fn queue_clip(&mut self, clip: &AudioClip) {
        let pcm = resample_to_8khz_u8(clip);
        self.queue_pcm_samples(&pcm);
    }

    pub fn queue_villain_speech(&mut self, villain_id: u16, duration_secs: f32) {
        let total_samples = (Self::SAMPLE_RATE as f32 * duration_secs) as usize;
        let mut pcm = Vec::with_capacity(total_samples);
        let base_freq = match villain_id {
            1010 => 350.0,
            1002 => 140.0,
            _ => 220.0,
        };

        for i in 0..total_samples {
            let t = i as f32 / Self::SAMPLE_RATE as f32;
            let sample = ((t * base_freq * std::f32::consts::TAU).sin() * 64.0 + 128.0) as u8;
            pcm.push(sample);
        }

        self.queue_pcm_samples(&pcm);
    }

    pub fn pop_packet(&mut self) -> Option<[u8; REPORT_SIZE]> {
        self.queue.pop_front()
    }

    pub fn is_streaming(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn pending_packets(&self) -> usize {
        self.queue.len()
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl Default for PortalSpeakerSink {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SystemAudioSink {
    _stream: Option<OutputStream>,
    _handle: Option<OutputStreamHandle>,
    bgm_sink: Option<Sink>,
    sfx_sink: Option<Sink>,
    bgm_volume: f32,
    sfx_volume: f32,
}

impl SystemAudioSink {
    pub fn new() -> Self {
        let (stream, handle, bgm_sink, sfx_sink) = match OutputStream::try_default() {
            Ok((st, hd)) => {
                let bgm = Sink::try_new(&hd).ok();
                let sfx = Sink::try_new(&hd).ok();
                (Some(st), Some(hd), bgm, sfx)
            }
            Err(_) => (None, None, None, None),
        };

        Self {
            _stream: stream,
            _handle: handle,
            bgm_sink,
            sfx_sink,
            bgm_volume: 0.7,
            sfx_volume: 1.0,
        }
    }

    pub fn play_sfx_tone(&self, freq_hz: f32, duration_secs: f32) {
        if let Some(sink) = &self.sfx_sink {
            let source = rodio::source::SineWave::new(freq_hz)
                .take_duration(std::time::Duration::from_secs_f32(duration_secs))
                .amplify(self.sfx_volume);
            sink.append(source);
        }
    }

    pub fn play_clip(&self, clip: &AudioClip) {
        if let Some(sink) = &self.sfx_sink {
            let source = rodio::buffer::SamplesBuffer::new(
                clip.channels,
                clip.sample_rate,
                clip.samples.clone(),
            )
            .amplify(self.sfx_volume);
            sink.append(source);
        }
    }

    pub fn set_bgm_volume(&mut self, vol: f32) {
        self.bgm_volume = vol.clamp(0.0, 1.0);
        if let Some(sink) = &self.bgm_sink {
            sink.set_volume(self.bgm_volume);
        }
    }

    pub fn set_sfx_volume(&mut self, vol: f32) {
        self.sfx_volume = vol.clamp(0.0, 1.0);
        if let Some(sink) = &self.sfx_sink {
            sink.set_volume(self.sfx_volume);
        }
    }
}

impl Default for SystemAudioSink {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AudioEngine {
    pub system: SystemAudioSink,
    pub portal: PortalSpeakerSink,
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {
            system: SystemAudioSink::new(),
            portal: PortalSpeakerSink::new(),
        }
    }

    pub fn on_villain_trapped(&mut self, villain_id: u16) {
        self.system.play_sfx_tone(440.0, 0.3);
        self.portal.queue_villain_speech(villain_id, 1.0);
    }

    pub fn play_villain_clip(&mut self, clip: &AudioClip) {
        self.portal.queue_clip(clip);
        self.system.play_clip(clip);
    }

    pub fn on_hero_summoned(&mut self) {
        self.system.play_sfx_tone(880.0, 0.2);
    }

    pub fn on_pickup_collected(&mut self) {
        self.system.play_sfx_tone(1320.0, 0.12);
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portal_speaker_packet_queue() {
        let mut speaker = PortalSpeakerSink::new();
        assert!(!speaker.is_streaming());

        speaker.queue_villain_speech(1010, 0.1);
        assert!(speaker.is_streaming());
        assert!(speaker.pending_packets() > 0);

        let first = speaker.pop_packet().expect("First packet");
        assert_eq!(first[0], b'V');
        assert_eq!(first[1], 0);

        let second = speaker.pop_packet().expect("Second packet");
        assert_eq!(second[0], b'V');
        assert_eq!(second[1], 1);
    }

    #[test]
    fn test_system_audio_creation() {
        let audio = AudioEngine::new();
        assert_eq!(audio.portal.pending_packets(), 0);
    }

    #[test]
    fn test_resample_and_queue_clip() {
        let clip = AudioClip {
            channels: 1,
            sample_rate: 16000,
            samples: vec![0, 32767, -32768, 0],
        };
        let pcm = resample_to_8khz_u8(&clip);
        assert_eq!(pcm.len(), 2);
        let mut speaker = PortalSpeakerSink::new();
        speaker.queue_clip(&clip);
        assert!(speaker.is_streaming());
    }
}
