struct DecoderSettings {
    /// How many ms to pre-buffer
    buffer_len_ms: usize,
    /// Max CPU load in percent on the decoder thread
    max_cpu_load: usize,
}