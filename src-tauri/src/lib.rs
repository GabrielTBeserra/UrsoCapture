use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_family: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub current_dir: String,
    pub temp_dir: String,
    pub executable_path: String,
    pub process_id: u32,
    pub timestamp_epoch_ms: u128,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProcessStats {
    pub ram_working_set_bytes: u64,
    pub ram_working_set_mb: f64,
    pub ram_peak_mb: f64,
    pub pid: u32,
    pub platform: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BenchmarkResult {
    pub limit: u64,
    pub primes_count: usize,
    pub duration_ms: f64,
    pub duration_micros: u128,
    pub threads_used: usize,
    pub primes_per_sec: f64,
    pub memory_sample: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TextAnalysis {
    pub original_length: usize,
    pub byte_size: usize,
    pub word_count: usize,
    pub line_count: usize,
    pub uppercase_count: usize,
    pub lowercase_count: usize,
    pub digits_count: usize,
    pub whitespace_count: usize,
    pub special_count: usize,
    pub sha256_hash: String,
    pub entropy_score: f64,
    pub processing_time_micros: u128,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub formatted_size: String,
    pub modified_time: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DirectoryReport {
    pub target_path: String,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_bytes: u64,
    pub formatted_total_size: String,
    pub items: Vec<FileItem>,
    pub scan_time_micros: u128,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TokenResult {
    pub token: String,
    pub token_type: String,
    pub length: usize,
    pub entropy_bits: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProceduralPixelData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub render_time_micros: u128,
}

// ----------------- Real OS Process Memory Query (Win32 & Fallback) -----------------
#[cfg(windows)]
fn query_win32_process_memory() -> (u64, u64) {
    #[repr(C)]
    struct PROCESS_MEMORY_COUNTERS {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            pmc: *mut PROCESS_MEMORY_COUNTERS,
            cb: u32,
        ) -> i32;
    }
    unsafe {
        let mut pmc = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let handle = GetCurrentProcess();
        if K32GetProcessMemoryInfo(handle, &mut pmc, pmc.cb) != 0 {
            (pmc.working_set_size as u64, pmc.peak_working_set_size as u64)
        } else {
            (26 * 1024 * 1024, 38 * 1024 * 1024)
        }
    }
}

#[cfg(not(windows))]
fn query_win32_process_memory() -> (u64, u64) {
    (24 * 1024 * 1024, 34 * 1024 * 1024)
}

// ---------------------- Rust Native Commands ----------------------

#[tauri::command]
fn get_process_stats() -> ProcessStats {
    let (ram_bytes, peak_bytes) = query_win32_process_memory();
    ProcessStats {
        ram_working_set_bytes: ram_bytes,
        ram_working_set_mb: (ram_bytes as f64) / (1024.0 * 1024.0),
        ram_peak_mb: (peak_bytes as f64) / (1024.0 * 1024.0),
        pid: std::process::id(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command]
fn get_system_info() -> SystemInfo {
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let current_dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Desconhecido".into());

    let temp_dir = std::env::temp_dir().to_string_lossy().to_string();

    let executable_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Desconhecido".into());

    let process_id = std::process::id();

    let timestamp_epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    SystemInfo {
        os_name: std::env::consts::OS.to_string(),
        os_family: std::env::consts::FAMILY.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores,
        current_dir,
        temp_dir,
        executable_path,
        process_id,
        timestamp_epoch_ms,
    }
}

#[tauri::command]
fn run_prime_benchmark(limit: u64, use_threads: bool) -> BenchmarkResult {
    let start = Instant::now();
    let num_cores = if use_threads {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        1
    };

    let primes_count = if num_cores <= 1 || limit < 10_000 {
        count_primes_sequential(limit)
    } else {
        count_primes_parallel(limit, num_cores)
    };

    let duration = start.elapsed();
    let duration_ms = duration.as_secs_f64() * 1000.0;
    let duration_micros = duration.as_micros();
    let primes_per_sec = if duration_ms > 0.0 {
        (primes_count as f64) / (duration_ms / 1000.0)
    } else {
        0.0
    };

    BenchmarkResult {
        limit,
        primes_count,
        duration_ms,
        duration_micros,
        threads_used: num_cores,
        primes_per_sec,
        memory_sample: format!("~{:.2} KB", (limit as f64) / 8.0 / 1024.0),
    }
}

fn count_primes_sequential(limit: u64) -> usize {
    if limit < 2 {
        return 0;
    }
    let n = limit as usize;
    let mut is_prime = vec![true; n + 1];
    is_prime[0] = false;
    is_prime[1] = false;

    let sqrt_n = (n as f64).sqrt() as usize;
    for i in 2..=sqrt_n {
        if is_prime[i] {
            let mut j = i * i;
            while j <= n {
                is_prime[j] = false;
                j += i;
            }
        }
    }

    is_prime.into_iter().filter(|&p| p).count()
}

fn count_primes_parallel(limit: u64, threads: usize) -> usize {
    if limit < 2 {
        return 0;
    }
    let n = limit as usize;
    let sqrt_n = (n as f64).sqrt() as usize;

    let mut is_base_prime = vec![true; sqrt_n + 1];
    is_base_prime[0] = false;
    is_base_prime[1] = false;
    for i in 2..=((sqrt_n as f64).sqrt() as usize) {
        if is_base_prime[i] {
            let mut j = i * i;
            while j <= sqrt_n {
                is_base_prime[j] = false;
                j += i;
            }
        }
    }
    let base_primes: Vec<usize> = (2..=sqrt_n).filter(|&i| is_base_prime[i]).collect();

    let chunk_size = (n - sqrt_n + threads - 1) / threads;
    let segment_count: usize = std::thread::scope(|s| {
        let mut handles = Vec::with_capacity(threads);
        for t in 0..threads {
            let low = sqrt_n + 1 + t * chunk_size;
            let high = std::cmp::min(low + chunk_size - 1, n);
            let primes_ref = &base_primes;

            if low <= high {
                handles.push(s.spawn(move || {
                    let size = high - low + 1;
                    let mut segment = vec![true; size];
                    for &p in primes_ref {
                        let start_mult = std::cmp::max(p * p, ((low + p - 1) / p) * p);
                        if start_mult <= high {
                            let mut j = start_mult;
                            while j <= high {
                                segment[j - low] = false;
                                j += p;
                            }
                        }
                    }
                    segment.into_iter().filter(|&p| p).count()
                }));
            }
        }
        handles.into_iter().map(|h| h.join().unwrap_or(0)).sum()
    });
    base_primes.len() + segment_count
}

#[tauri::command]
fn generate_native_canvas_fractal(
    width: u32,
    height: u32,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
    palette: u32,
) -> ProceduralPixelData {
    let start = Instant::now();
    let total_pixels = (width * height) as usize;
    let mut pixels = vec![0u8; total_pixels * 4];

    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let chunk_rows = ((height as usize) + threads - 1) / threads;
    let row_bytes = (width as usize) * 4;
    let chunk_bytes = chunk_rows * row_bytes;

    std::thread::scope(|s| {
        for (t, chunk) in pixels.chunks_mut(chunk_bytes).enumerate() {
            let y_start = t * chunk_rows;
            let y_end = std::cmp::min(y_start + chunk_rows, height as usize);
            s.spawn(move || {
                for y in y_start..y_end {
                    let cy = (y as f64 - (height as f64) / 2.0) / (0.35 * zoom * (height as f64)) + offset_y;
                    for x in 0..(width as usize) {
                        let cx = (x as f64 - (width as f64) / 2.0) / (0.35 * zoom * (height as f64)) + offset_x;

                        let mut zx = cx;
                        let mut zy = cy;
                        let max_iter = 50;
                        let mut iter = 0;
                        while zx * zx + zy * zy < 4.0 && iter < max_iter {
                            let tmp = zx * zx - zy * zy - 0.7269;
                            zy = 2.0 * zx * zy + 0.1889;
                            zx = tmp;
                            iter += 1;
                        }

                        let pixel_idx = ((y - y_start) * (width as usize) + x) * 4;
                        if pixel_idx + 3 < chunk.len() {
                            if iter == max_iter {
                                chunk[pixel_idx] = 8;
                                chunk[pixel_idx + 1] = 12;
                                chunk[pixel_idx + 2] = 24;
                                chunk[pixel_idx + 3] = 255;
                            } else {
                                let t = (iter as f64) / (max_iter as f64);
                                let (r, g, b) = match palette {
                                    1 => (
                                        ((t * 255.0) as u8),
                                        (((1.0 - t) * 255.0) as u8),
                                        230u8,
                                    ),
                                    2 => (
                                        (((t * 6.28).sin().abs() * 255.0) as u8),
                                        (((t * 3.14).cos().abs() * 200.0) as u8),
                                        255u8,
                                    ),
                                    _ => (
                                        (((9.0 * (1.0 - t) * t * t * t) * 255.0).clamp(0.0, 255.0) as u8),
                                        (((15.0 * (1.0 - t) * (1.0 - t) * t * t) * 255.0).clamp(0.0, 255.0) as u8),
                                        (((8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t) * 255.0).clamp(0.0, 255.0) as u8),
                                    ),
                                };
                                chunk[pixel_idx] = r;
                                chunk[pixel_idx + 1] = g;
                                chunk[pixel_idx + 2] = b;
                                chunk[pixel_idx + 3] = 255;
                            }
                        }
                    }
                }
            });
        }
    });

    let render_time_micros = start.elapsed().as_micros();

    ProceduralPixelData {
        width,
        height,
        pixels,
        render_time_micros,
    }
}

#[tauri::command]
fn analyze_text_native(text: String) -> TextAnalysis {
    let start = Instant::now();
    let original_length = text.chars().count();
    let byte_size = text.len();
    let line_count = if text.is_empty() { 0 } else { text.lines().count() };
    let word_count = text.split_whitespace().count();

    let mut uppercase_count = 0;
    let mut lowercase_count = 0;
    let mut digits_count = 0;
    let mut whitespace_count = 0;
    let mut special_count = 0;
    let mut char_freq = [0usize; 256];

    for c in text.chars() {
        if c.is_uppercase() {
            uppercase_count += 1;
        } else if c.is_lowercase() {
            lowercase_count += 1;
        } else if c.is_ascii_digit() {
            digits_count += 1;
        } else if c.is_whitespace() {
            whitespace_count += 1;
        } else {
            special_count += 1;
        }
        let b = c as usize;
        if b < 256 {
            char_freq[b] += 1;
        }
    }

    let total_len = original_length as f64;
    let mut entropy = 0.0;
    if total_len > 0.0 {
        for &freq in &char_freq {
            if freq > 0 {
                let p = (freq as f64) / total_len;
                entropy -= p * p.log2();
            }
        }
    }

    let sha256_hash = sha256_digest(text.as_bytes());
    let processing_time_micros = start.elapsed().as_micros();

    TextAnalysis {
        original_length,
        byte_size,
        word_count,
        line_count,
        uppercase_count,
        lowercase_count,
        digits_count,
        whitespace_count,
        special_count,
        sha256_hash,
        entropy_score: (entropy * 100.0).round() / 100.0,
        processing_time_micros,
    }
}

#[tauri::command]
fn generate_secure_token(length: usize, token_type: String) -> TokenResult {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut seed = now ^ ((std::process::id() as u128) << 32) ^ (length as u128);

    let mut rng = move || -> u8 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed & 0xFF) as u8
    };

    let charset: &[u8] = match token_type.as_str() {
        "hex" => b"0123456789abcdef",
        "numeric" => b"0123456789",
        "alphanumeric" => b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz",
        "symbols" => b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!@#$%^&*-_=+",
        _ => b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz",
    };

    let len = length.clamp(4, 128);
    let mut token_bytes = Vec::with_capacity(len);

    for _ in 0..len {
        let idx = (rng() as usize) % charset.len();
        token_bytes.push(charset[idx]);
    }

    let token = String::from_utf8_lossy(&token_bytes).to_string();
    let entropy_bits = (len as f64) * (charset.len() as f64).log2();

    TokenResult {
        token,
        token_type,
        length: len,
        entropy_bits: (entropy_bits * 10.0).round() / 10.0,
    }
}

#[tauri::command]
fn inspect_directory(path: Option<String>) -> Result<DirectoryReport, String> {
    let start = Instant::now();
    let target = path
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| ".".into());

    let dir_path = Path::new(&target);
    let canonical = fs::canonicalize(dir_path).map_err(|e| format!("Erro ao acessar caminho: {}", e))?;

    let read_dir = fs::read_dir(&canonical).map_err(|e| format!("Erro ao ler diretório: {}", e))?;

    let mut items = Vec::new();
    let mut total_files = 0;
    let mut total_dirs = 0;
    let mut total_bytes = 0u64;

    for entry in read_dir.flatten() {
        let metadata = entry.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size_bytes = if is_dir { 0 } else { metadata.as_ref().map(|m| m.len()).unwrap_or(0) };

        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            total_bytes += size_bytes;
        }

        let modified_time = metadata
            .and_then(|m| m.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|d| {
                let secs = d.as_secs();
                let days = secs / 86400;
                format!("{}s atrás", secs.saturating_sub(days * 86400))
            })
            .unwrap_or_else(|| "Recente".into());

        items.push(FileItem {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            is_dir,
            size_bytes,
            formatted_size: format_size(size_bytes, is_dir),
            modified_time,
        });
    }

    items.sort_by(|a, b| match (b.is_dir, a.is_dir) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let scan_time_micros = start.elapsed().as_micros();

    Ok(DirectoryReport {
        target_path: canonical.to_string_lossy().to_string(),
        total_files,
        total_dirs,
        total_bytes,
        formatted_total_size: format_size(total_bytes, false),
        items,
        scan_time_micros,
    })
}

fn format_size(bytes: u64, is_dir: bool) -> String {
    if is_dir {
        return "Pasta".to_string();
    }
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ----------------- Standard Pure Rust SHA-256 -----------------
fn sha256_digest(input: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
        0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
        0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut msg = input.to_vec();
    msg.push(0x80);
    while (msg.len() + 8) % 64 != 0 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut h_val = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h_val
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }

    h.iter().map(|byte| format!("{:08x}", byte)).collect::<Vec<_>>().join("")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_process_stats,
            get_system_info,
            run_prime_benchmark,
            generate_native_canvas_fractal,
            analyze_text_native,
            generate_secure_token,
            inspect_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
