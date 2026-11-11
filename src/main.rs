use serde_json::Value;
use std::{env, fs, hint::black_box, process::ExitCode, thread, time::{Duration, Instant}};

fn validate_job(v: &Value) -> Result<(usize, u64), String> {
    let obj = v.as_object().ok_or("Dữ liệu phải là JSON object")?;
    let id = obj.get("jobId").and_then(Value::as_str).ok_or("Thiếu jobId")?;
    if id.trim().is_empty() || id.len() > 256 { return Err("jobId không hợp lệ".into()); }
    let copies = obj.get("copies").and_then(Value::as_u64).ok_or("Thiếu copies")?;
    if !(1..=1000).contains(&copies) { return Err("copies ngoài khoảng 1..1000".into()); }
    let cards = obj.get("cards").and_then(Value::as_array).ok_or("Thiếu cards")?;
    if cards.is_empty() || cards.len() > 10000 { return Err("cards phải có 1..10000 phần tử".into()); }
    for (i, card) in cards.iter().enumerate() {
        let item = card.as_object().ok_or_else(|| format!("cards[{i}] không phải object"))?;
        for key in ["name", "color", "quantity"] {
            let text = item.get(key).and_then(Value::as_str).ok_or_else(|| format!("cards[{i}].{key} phải là chuỗi"))?;
            if text.trim().is_empty() || text.len() > 4096 {
                return Err(format!("cards[{i}].{key} rỗng hoặc quá dài"));
            }
        }
    }
    if let Some(cells) = obj.get("cells") {
        if !cells.is_object() { return Err("cells phải là object".into()); }
    }
    if let Some(n) = obj.get("startNumber") {
        if n.as_u64().is_none() { return Err("startNumber phải là số nguyên không âm".into()); }
    }
    Ok((cards.len(), copies))
}

const SAMPLE: &str = r#"{"jobId":"rust-test-001","copies":1,"cards":[{"name":"部材 A","color":"白","quantity":"100"}],"cells":{},"startNumber":1}"#;

fn simulate(path: &str) -> Result<(), String> {
    let data = fs::read_to_string(path).map_err(|e| format!("Không đọc được file: {e}"))?;
    let value: Value = serde_json::from_str(&data).map_err(|e| format!("JSON không hợp lệ: {e}"))?;
    let (cards, copies) = validate_job(&value)?;
    println!("DRY RUN OK: {cards} cards, {copies} copies. KHÔNG GỬI LỆNH IN.");
    Ok(())
}

fn benchmark(iterations: usize) -> Result<(), String> {
    if iterations == 0 || iterations > 10_000_000 { return Err("Số vòng cần từ 1 đến 10000000".into()); }
    let start = Instant::now();
    let mut count = 0usize;
    for _ in 0..iterations {
        let v: Value = serde_json::from_str(black_box(SAMPLE)).map_err(|e| e.to_string())?;
        let (cards, _) = validate_job(&v)?;
        count += black_box(cards);
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("BENCHMARK (JSON parse + validation, KHÔNG IN): {iterations} vòng; {ms:.2} ms; {:.2} microseconds/vòng; {count} cards", ms * 1000.0 / iterations as f64);
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--simulate") => simulate(args.get(2).ok_or("Cần đường dẫn sample-job.json")?),
        Some("--benchmark") => {
            let n = args.get(2).map(String::as_str).unwrap_or("100000").parse::<usize>().map_err(|e| e.to_string())?;
            benchmark(n)
        }
        Some("--idle-seconds") => {
            let seconds = args.get(2).ok_or("Cần số giây")?.parse::<u64>().map_err(|e| e.to_string())?;
            if seconds > 3600 { return Err("Tối đa 3600 giây".into()); }
            println!("Đang chờ {seconds} giây để đo RAM; không kết nối mạng / không in.");
            thread::sleep(Duration::from_secs(seconds));
            Ok(())
        }
        _ => {
            println!("Fujifilm Print Rust PROTOTYPE v0.1.0 — không kết nối Cloudflare, không in.");
            println!("  --simulate <sample-job.json> | --benchmark [100000] | --idle-seconds <30>");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("LỖI: {e}"); ExitCode::FAILURE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_job() {
        let v: Value = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(validate_job(&v), Ok((1, 1)));
    }
    #[test]
    fn missing_cards_is_error() {
        let v: Value = serde_json::json!({"jobId":"a", "copies":1});
        assert!(validate_job(&v).is_err());
    }
    #[test]
    fn zero_copies_is_error() {
        let v: Value = serde_json::json!({"jobId":"a", "copies":0,"cards":[{"name":"a","color":"b","quantity":"1"}]});
        assert!(validate_job(&v).is_err());
    }
    #[test]
    fn japanese_cards_are_accepted() {
        let v: Value = serde_json::json!({"jobId":"印刷-1","copies":2,"cards":[{"name":"部材","color":"白","quantity":"100"}],"cells":{}});
        assert_eq!(validate_job(&v), Ok((1, 2)));
    }
}
