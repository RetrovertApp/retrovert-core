#include <stdint.h>

#define STB_SPRINTF_IMPLEMENTATION
#include "stb_sprintf.h"

void rust_log_callback(void* rust_data, int level, char* buffer, int buffer_size, const char* file, int line);

///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

void c_log_func(void* priv_data, uint32_t level, const char* file, int line, const char* fmt, ...) {
    char output_buffer[8192];

    va_list ap;

    va_start(ap, fmt);
    int count = stbsp_vsnprintf(output_buffer, sizeof(output_buffer), fmt, ap);
    va_end(ap);

    // Call to Rust
    rust_log_callback(priv_data, (uint32_t)level, output_buffer, count, file, line);
}
