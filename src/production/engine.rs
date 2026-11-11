use super::{Config,Job,agent_id,log};
use std::{fs,io::Read,path::{Path,PathBuf},process::{Command,Stdio},thread,time::{Duration,Instant}};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::sync::atomic::{AtomicU64,Ordering};
static LAST_HEARTBEAT_LOG:AtomicU64=AtomicU64::new(0);

// Không mở cửa sổ console cho chương trình con của Print Agent.
#[cfg(windows)]
fn hide_child_window(cmd:&mut Command){
 use std::os::windows::process::CommandExt;
 cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
fn hide_child_window(_cmd:&mut Command){}

pub fn pdf_output(c:&Config,j:&Job)->Result<(),String>{
 j.check()?;let template=c.path(&c.template_path);
 if !template.is_file()||template.extension().and_then(|x|x.to_str()).map_or(true,|x|!x.eq_ignore_ascii_case("xlsx")){return Err(format!("Không tìm thấy file XLSX: {}",template.display()))}
 let folder=if c.pdf_output_path.trim().is_empty(){std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(||Config::base_dir()).join("Documents").join("Machine-Control-PDF")}else{let p=PathBuf::from(&c.pdf_output_path);if !p.is_absolute(){return Err("PdfOutputPath phải là đường dẫn tuyệt đối".into())}p};
 fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
 let mode=if c.print_mode=="printer"{"pdfprint"}else{&c.print_mode};
 let printer=if mode=="pdfprint"{Some(resolve_sumatra(c)?)}else{None};
 // Windows PowerShell 5.1 doc UTF-8 không BOM dưới dạng ANSI: lỗi ký tự Tiếng Việt khiến parser ngắt chuỗi.
 // Luôn tạo bản script chuẩn UTF-8 BOM từ mã nguồn đã nhúng, không dùng bản cũ từng được tạo lỗi.
 let script=Config::base_dir().join("scripts").join("export-excel-pdf.ps1");
 if let Some(parent)=script.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
 let embedded=include_str!("../../scripts/export-excel-pdf.ps1").as_bytes();
 let mut script_bytes=Vec::with_capacity(embedded.len()+3);
 script_bytes.extend_from_slice(&[0xEF,0xBB,0xBF]);
 script_bytes.extend_from_slice(embedded);
 let replace=fs::read(&script).map_or(true,|existing|existing!=script_bytes);
 if replace {
  let tmp=script.with_extension("ps1.new");
  fs::write(&tmp,&script_bytes).map_err(|e|format!("Không tạo script UTF-8 BOM: {e}"))?;
  if script.exists(){fs::remove_file(&script).map_err(|e|format!("Không thay được script Excel lỗi cũ: {e}"))?;}
  fs::rename(&tmp,&script).map_err(|e|format!("Không thay được script Excel: {e}"))?;
  log("Đã làm mới script Excel chuẩn UTF-8 BOM, thay thế file script cũ");
 }
 let per=c.max_pdf_pages*8;
 let chunks=if j.cards.is_empty(){1}else{j.cards.len().div_ceil(per)};
 for copy in 0..j.copies{
  for chunk in 0..chunks{
   let offset=chunk*per;
   let part=j.part(offset,per,copy);
   let temp=std::env::temp_dir().join(format!("fujifilm-rust-{}.json",uuid::Uuid::new_v4()));
   let bytes=serde_json::to_vec(&part).map_err(|e|e.to_string())?;
   fs::write(&temp,bytes).map_err(|e|e.to_string())?;
   // Mỗi lần xuất dùng file riêng để không xóa/ghi đè PDF cũ còn mở bởi Adobe, Sumatra hoặc OneDrive.
   // Không dùng jobId trực tiếp trong tên file nhằm tránh ký tự đường dẫn không an toàn.
   let pdf=folder.join(format!("print-{}.pdf",uuid::Uuid::new_v4()));
   // CREATE_NO_WINDOW giữ PowerShell hoàn toàn ẩn khi Agent chạy bằng EXE GUI.
   // -WindowStyle Hidden chỉ là lớp bảo vệ bổ sung cho Windows PowerShell 5.1.
   let mut ps=Command::new("powershell.exe");
   ps.args(["-NoProfile","-NonInteractive","-WindowStyle","Hidden","-ExecutionPolicy","Bypass","-File"])
    .arg(&script).arg("-JobFile").arg(&temp).arg("-Template").arg(&template).arg("-Output").arg(&pdf)
    .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
   hide_child_window(&mut ps);
   let result=ps.output();
   let _=fs::remove_file(&temp);
   let result=result.map_err(|e|format!("Không khởi động Excel PowerShell: {e}"))?;
   if !result.status.success(){return Err(format!("Xuất PDF thất bại: {}",String::from_utf8_lossy(&result.stderr)))}
   let mut file=fs::File::open(&pdf).map_err(|e|format!("PDF chưa tạo: {e}"))?;let mut sig=[0u8;5];file.read_exact(&mut sig).map_err(|e|e.to_string())?;
   if &sig!=b"%PDF-"{return Err("PDF sai định dạng; không in".into())}
   if let Some(ref exe)=printer{
    let mut cmd=Command::new(exe);
    if c.printer_name.trim().is_empty(){cmd.arg("-print-to-default");}else{cmd.args(["-print-to",&c.printer_name]);}
    cmd.args(["-print-settings","noscale,simplex","-silent"]).arg(&pdf)
       .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    hide_child_window(&mut cmd);
    let status=cmd.spawn().map_err(|e|format!("Không mở được SumatraPDF: {e}"))?
     .wait().map_err(|e|format!("SumatraPDF lỗi: {e}"))?;
    if !status.success(){return Err(format!("SumatraPDF trả mã {:?}; không thử lại để tránh in trùng",status.code()))}
   }
   log(&format!("Đã xuất PDF part {}/{} copy {}/{}: {}",chunk+1,chunks,copy+1,j.copies,pdf.display()));
  }
 }
 Ok(())
}
fn resolve_sumatra(c:&Config)->Result<PathBuf,String>{
 let mut paths=vec![c.path(&c.pdf_printer_exe)];
 if !Path::new(&c.pdf_printer_exe).is_absolute(){
  for key in ["ProgramFiles","ProgramFiles(x86)","LOCALAPPDATA"]{if let Some(p)=std::env::var_os(key){paths.push(PathBuf::from(p).join("SumatraPDF").join(&c.pdf_printer_exe));}}
 }
 paths.into_iter().find(|p|p.is_file()&&p.extension().and_then(|e|e.to_str()).is_some_and(|e|e.eq_ignore_ascii_case("exe"))).ok_or("Không tìm thấy SumatraPDF.exe. Hãy cài trong Print Settings".into())
}

pub struct Api{base:String,token:String,id:String,client:ureq::Agent}
impl Api{
 pub fn new(c:&Config,id:String)->Result<Self,String>{
  let url=c.api_base_url.trim_end_matches('/');
  if !(url.starts_with("https://")||url.starts_with("http://localhost:")||url.starts_with("http://127.0.0.1:")){return Err("ApiBaseUrl phải là HTTPS (hoặc localhost)".into())}
  if c.agent_token.is_empty(){return Err("AgentToken trống".into())}
  Ok(Self{base:url.to_string(),token:c.agent_token.clone(),id,client:ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build()})
 }
 fn request(&self,path:&str)->ureq::Request{
  self.client.get(&format!("{}/{}",self.base,path)).set("Authorization",&format!("Bearer {}",self.token)).set("X-Print-Agent-Id",&self.id)
 }
 fn post(&self,path:&str,v:Value)->Result<(),String>{
  let res=self.client.post(&format!("{}/{}",self.base,path)).set("Authorization",&format!("Bearer {}",self.token)).set("X-Print-Agent-Id",&self.id).send_json(v);
  match res{Ok(_)=>Ok(()),Err(e)=>Err(format!("POST {path}: {e}"))}
 }
 fn get(&self,path:&str)->Result<Option<Value>,String>{
  match self.request(path).call(){Ok(res)=>{
    if res.status()==204{return Ok(None)}
    let v=res.into_json::<Value>().map_err(|e|e.to_string())?;Ok(Some(v))
   },Err(ureq::Error::Status(204,_))=>Ok(None),Err(e)=>Err(format!("GET {path}: {e}"))}
 }
 fn get_bytes(&self,path:&str,limit:usize)->Result<Vec<u8>,String>{
  let res=self.request(path).call().map_err(|e|e.to_string())?;
  let mut data=Vec::new();res.into_reader().take((limit+1) as u64).read_to_end(&mut data).map_err(|e|e.to_string())?;
  if data.len()>limit{return Err("File tải vượt giới hạn".into())}Ok(data)
 }
 pub fn snapshot(&self,c:&Config){if let Err(e)=self.post("api/print-agent/config/snapshot",json!({"config":c.snapshot()})){log(&e)}}
 pub fn heartbeat(&self,c:&Config){
  let (name,valid,paper,windows_status)=super::status::printer_status(c);
  let machine=std::env::var("COMPUTERNAME").unwrap_or_else(|_|"Windows".into());
  
  let body=json!({"agentId":self.id,"machineName":machine,"printerName":name,
    "printerDisplayName":c.printer_display_name,"agentPlatform":"rust","agentVersion":option_env!("FUJIFILM_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    "printerValid":valid,"paperStatus":paper,"windowsStatus":windows_status});
  match self.post("api/print-agent/status/heartbeat",body){
   Ok(())=>{
    // Heartbeat chi la tin hieu online, KHONG PHAI JOB. Ghi lan dau va moi 15 phut de log khong bi ngap.
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let previous=LAST_HEARTBEAT_LOG.load(Ordering::Relaxed);
    if now.saturating_sub(previous)>=900 && LAST_HEARTBEAT_LOG.compare_exchange(previous,now,Ordering::Relaxed,Ordering::Relaxed).is_ok(){
     log(&format!("STATUS ONLINE (khong phai job) agent={} printer={} valid={}",self.id,name,valid));
    }
   },
   Err(e)=>log(&format!("HEARTBEAT_FAILED: {e}")),
  }
  let installed=Config::base_dir().join("SumatraPDF.exe").is_file();
  if let Err(e)=self.post("api/print-agent/pdf-tool/status",json!({"installed":installed})){log(&e)}
 }
 pub fn config_command(&self,c:&mut Config){
  let Ok(Some(cmd))=self.get("api/print-agent/config/command")else{return};
  if cmd["update"]!=true{return}
  let revision=cmd["revision"].as_str().unwrap_or("");
  if uuid::Uuid::parse_str(revision).is_err(){return}
  let result=(||->Result<(),String>{
   let cfg=&cmd["config"];let fields=["PrinterName","PrinterDisplayName","TemplatePath","PrintMode","PdfOutputPath","PdfPrinterExe","PollSeconds","MaxPdfPages","AutoUpdateEnabled"];
   if cfg.as_object().map_or(true,|o|o.len()!=fields.len()||o.keys().any(|k|!fields.contains(&k.as_str()))){return Err("Remote config sai cấu trúc".into())}
   let mut next=c.clone();
   let s=|key:&str|->Result<String,String>{let value=cfg[key].as_str().ok_or(format!("{key} phải là chuỗi"))?;if value.len()>512||value.contains(['\r','\n']){return Err(format!("{key} quá dài hoặc chứa xuống dòng"))}Ok(value.into())};
   next.printer_name=s("PrinterName")?;next.printer_display_name=s("PrinterDisplayName")?;next.template_path=s("TemplatePath")?;
   next.print_mode=s("PrintMode")?;next.pdf_output_path=s("PdfOutputPath")?;next.pdf_printer_exe=s("PdfPrinterExe")?;
   next.poll_seconds=cfg["PollSeconds"].as_u64().ok_or("PollSeconds sai")?;
   next.max_pdf_pages=cfg["MaxPdfPages"].as_u64().ok_or("MaxPdfPages sai")? as usize;
   next.auto_update_enabled=cfg["AutoUpdateEnabled"].as_bool().ok_or("AutoUpdateEnabled sai")?;
   next.check()?;
   if next.printer_name.is_empty()||!next.pdf_printer_exe.to_lowercase().ends_with(".exe"){return Err("Remote config thiếu máy in hoặc Sumatra.exe".into())}
   next.save(&Config::base_dir().join("config.ini"))?;*c=next;self.snapshot(c);Ok(())
  })();
  let (status,error)=match result {Ok(())=>("completed",None),Err(e)=>("failed",Some(e))};
  let _=self.post("api/print-agent/config/result",json!({"revision":revision,"status":status,"error":error,"config":if status=="completed"{c.snapshot()}else{Value::Null}}));
 }
 pub fn sumatra_command(&self){
  let Ok(Some(cmd))=self.get("api/print-agent/pdf-tool/command")else{return};
  if cmd["install"]!=true{return}
  let revision=cmd["revision"].as_str().unwrap_or("");
  if uuid::Uuid::parse_str(revision).is_err(){return}
  let result=self.install_sumatra();
  let status=if result.is_ok(){"completed"}else{"failed"};
  let _=self.post("api/print-agent/pdf-tool/result",json!({"revision":revision,"status":status,"error":result.err()}));
 }
 fn install_sumatra(&self)->Result<(),String>{
  let dest=Config::base_dir().join("SumatraPDF.exe");
  if dest.is_file(){return Ok(())} // không ghi đè bản quản trị viên
  let m=self.get("api/print-agent/update/latest")?.ok_or("Thiếu manifest")?;
  let size=m["sumatraSize"].as_u64().ok_or("Không có sumatraSize")?;
  let hash=m["sumatraSha256"].as_str().ok_or("Không có sumatraSha256")?;
  if !(1024..=40*1024*1024).contains(&size)||!is_hash(hash){return Err("Manifest Sumatra không hợp lệ".into())}
  let bytes=self.get_bytes("api/print-agent/update/sumatra",40*1024*1024)?;
  if bytes.len() as u64!=size||format!("{:x}",Sha256::digest(&bytes))!=hash.to_lowercase(){return Err("SHA-256 Sumatra sai".into())}
  let tmp=dest.with_extension("exe.tmp");fs::write(&tmp,&bytes).map_err(|e|e.to_string())?;
  if dest.exists(){let _=fs::remove_file(tmp);}else{fs::rename(tmp,dest).map_err(|e|e.to_string())?;}Ok(())
 }
 pub fn update_command(&self){
  let Ok(Some(cmd))=self.get("api/print-agent/update/command")else{return};
  if cmd["update"]==true{
   // Không áp dụng gói C# lên Rust. Cần release manifest Rust riêng để tránh hỏng chương trình.
   let _=self.post("api/print-agent/update/command/result",json!({"status":"failed","error":"Rust test: không thể áp dụng gói cập nhật dành cho C#; cần manifest Rust riêng"}));
  }
 }
 pub fn report_job(&self,id:&str,status:&str,error:Option<String>){
  let path=format!("api/print-agent/jobs/{}/status",encode_path(id));
  if let Err(e)=self.post(&path,json!({"status":status,"error":error})){log(&format!("{e}; KHÔNG tự in lại để tránh trùng"))}
 }
 pub fn poll(&self,c:&Config)->Result<bool,String>{
  let res=self.request("api/print-agent/jobs/next").call();
  let data=match res{
   Ok(r) if r.status()==204=>return Ok(false),
   Ok(r)=>r.into_string().map_err(|e|e.to_string())?,
   Err(ureq::Error::Status(204,_))=>return Ok(false),
   Err(e)=>return Err(format!("Lỗi nhận job: {e}"))
  };
  let job=Job::parse(&data)?;
  log(&format!("Đang xử lý job {}",job.job_id));
  let result=pdf_output(c,&job);
  let status=if result.is_ok(){if c.print_mode=="pdf"{"pdf_saved"}else{"submitted"}}else{"failed"};
  let error=result.err();
  self.report_job(&job.job_id,status,error.clone());
  if let Some(e)=error{log(&format!("JOB THẤT BẠI {}: {e}",job.job_id))}
  Ok(true)
 }
}
fn is_hash(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())}
fn encode_path(s:&str)->String{let mut o=String::new();for b in s.bytes(){if b.is_ascii_alphanumeric()||b"-._~".contains(&b){o.push(b as char)}else{o.push_str(&format!("%{b:02X}"))}}o}

pub fn run(live:bool)->Result<(),String>{
 let mut c=Config::load(&Config::base_dir().join("config.ini"))?;
 let id=agent_id()?;
 if !live {log("Rust Agent chạy CHẾ ĐỘ AN TOÀN; dùng --enable-live để thực sự nhận lệnh từ Cloudflare");return Ok(())}
 // Mặc định khi chạy live sẽ in thật nếu nhận job. Chỉ bật trên PC kiểm thử.
 let api=Api::new(&c,id.clone())?;
 let active_path=Config::base_dir().join("rust-core-active.pid");
 fs::write(&active_path,std::process::id().to_string()).map_err(|e|e.to_string())?;
 struct ActiveGuard(PathBuf);
 impl Drop for ActiveGuard {fn drop(&mut self){let _=fs::remove_file(&self.0);}}
 let _active_guard=ActiveGuard(active_path);
 let wake=super::socket::start(&c,id);
 log("Kết nối Machine-Control Rust --enable-live");
 let stop_file=Config::base_dir().join("rust-stop-request.flag");
 // Không xóa cờ dừng ở Core: nếu người dùng bấm Kết thúc ngay lúc khởi động,
 // tray đã gửi cờ; xóa ở đây sẽ làm thất lạc yêu cầu.
 // Đăng ký máy in trước khi nhận job hoặc thực hiện kiểm tra phụ chậm.
 api.heartbeat(&c);
 api.snapshot(&c);
 let mut last_heartbeat=Instant::now();
 let mut last_config=Instant::now()-Duration::from_secs(16);
 let mut last_pdf=Instant::now()-Duration::from_secs(16);
 let mut last_update=Instant::now()-Duration::from_secs(31);
 loop{
  if stop_file.exists(){let _=fs::remove_file(&stop_file);log("Đã nhận lệnh thoát sau khi hoàn thành job");return Ok(())}
  if last_heartbeat.elapsed()>=Duration::from_secs(60){api.heartbeat(&c);last_heartbeat=Instant::now()}
  match api.poll(&c){Ok(true)=>continue,Ok(false)=>(),Err(e)=>log(&e)}
  if last_config.elapsed()>=Duration::from_secs(15){api.config_command(&mut c);last_config=Instant::now()}
  if last_pdf.elapsed()>=Duration::from_secs(15){api.sumatra_command();last_pdf=Instant::now()}
  if last_update.elapsed()>=Duration::from_secs(30){api.update_command();last_update=Instant::now()}
  let _=wake.recv_timeout(Duration::from_secs(c.poll_seconds));
 }
}
