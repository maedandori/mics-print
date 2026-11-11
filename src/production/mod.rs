pub mod status;
pub mod socket;
pub mod engine;
use serde::{Deserialize,Serialize};
use std::{collections::BTreeMap,fs,path::{Path,PathBuf}};

#[derive(Debug,Clone)]
pub struct Config {
 pub api_base_url:String,pub agent_token:String,pub printer_name:String,pub printer_display_name:String,
 pub template_path:String,pub pdf_output_path:String,pub pdf_printer_exe:String,pub print_mode:String,
 pub poll_seconds:u64,pub max_pdf_pages:usize,pub auto_update_enabled:bool
}
impl Default for Config {fn default()->Self {Self{api_base_url:String::new(),agent_token:String::new(),printer_name:String::new(),printer_display_name:String::new(),
 template_path:"template.xlsx".into(),pdf_output_path:String::new(),pdf_printer_exe:"SumatraPDF.exe".into(),print_mode:"pdfprint".into(),
 poll_seconds:3,max_pdf_pages:96,auto_update_enabled:true}}}
impl Config {
 pub fn load(path:&Path)->Result<Self,String> {
  let s=fs::read_to_string(path).map_err(|e|format!("Không đọc được config.ini: {e}"))?;
  let mut map=BTreeMap::new();
  for line in s.lines(){let line=line.trim();if line.is_empty()||line.starts_with([';','#','[']){continue}
   if let Some((k,v))=line.split_once('='){map.insert(k.trim().to_ascii_lowercase(),v.trim().to_string());}}
  let get=|k:&str|map.get(&k.to_ascii_lowercase()).cloned();
  let mut c=Self::default();
  c.api_base_url=get("ApiBaseUrl").unwrap_or_default();c.agent_token=get("AgentToken").unwrap_or_default();
  c.printer_name=get("PrinterName").unwrap_or_default();c.printer_display_name=get("PrinterDisplayName").unwrap_or_default();
  c.template_path=get("TemplatePath").unwrap_or(c.template_path);c.pdf_output_path=get("PdfOutputPath").unwrap_or_default();
  c.pdf_printer_exe=get("PdfPrinterExe").unwrap_or(c.pdf_printer_exe);c.print_mode=get("PrintMode").unwrap_or(c.print_mode);
  c.poll_seconds=get("PollSeconds").unwrap_or("3".into()).parse().map_err(|_|"PollSeconds không hợp lệ")?;
  c.max_pdf_pages=get("MaxPdfPages").unwrap_or("96".into()).parse().map_err(|_|"MaxPdfPages không hợp lệ")?;
  c.auto_update_enabled=get("AutoUpdateEnabled").as_deref()!=Some("0");
  c.check()?;Ok(c)
 }
 pub fn check(&self)->Result<(),String>{
  if !(2..=300).contains(&self.poll_seconds){return Err("PollSeconds phải trong 2..300".into())}
  if !(1..=100).contains(&self.max_pdf_pages){return Err("MaxPdfPages phải trong 1..100".into())}
  if !matches!(self.print_mode.as_str(),"pdf"|"pdfprint"|"printer"){return Err("PrintMode không hợp lệ".into())}
  if self.template_path.is_empty(){return Err("TemplatePath trống".into())}
  Ok(())
 }
 pub fn save(&self,path:&Path)->Result<(),String>{
  self.check()?;
  let content=format!("[PrintAgent]\nApiBaseUrl={}\nAgentToken={}\nPrinterName={}\nPrinterDisplayName={}\nTemplatePath={}\nPollSeconds={}\nMaxPdfPages={}\nPrintMode={}\nPdfOutputPath={}\nPdfPrinterExe={}\nAutoUpdateEnabled={}\n",self.api_base_url,self.agent_token,self.printer_name,self.printer_display_name,self.template_path,self.poll_seconds,self.max_pdf_pages,self.print_mode,self.pdf_output_path,self.pdf_printer_exe,if self.auto_update_enabled{"1"}else{"0"});
  if content.contains('\r'){return Err("Dữ liệu có ký tự xuống dòng không hợp lệ".into())}
  let tmp=path.with_extension("ini.tmp");fs::write(&tmp,content).map_err(|e|e.to_string())?;
  if path.exists(){let bak=path.with_extension("ini.bak");fs::copy(path,bak).map_err(|e|e.to_string())?;}
  // Windows không cho rename ghi đè file có sẵn: thực hiện thay thế bằng backup, rollback khi lỗi.
  if path.exists(){fs::remove_file(path).map_err(|e|format!("Không thể thay config.ini: {e}"))?;}
  match fs::rename(&tmp,path){Ok(())=>Ok(()),Err(e)=>{
   let backup=path.with_extension("ini.bak");if !path.exists()&&backup.exists(){let _=fs::copy(backup,path);}
   Err(format!("Không thay được config.ini: {e}"))
  }}
 }
 pub fn base_dir()->PathBuf{std::env::current_exe().ok().and_then(|p|p.parent().map(Path::to_path_buf)).unwrap_or_else(||PathBuf::from("."))}
 pub fn path(&self,s:&str)->PathBuf{let p=PathBuf::from(s);if p.is_absolute(){p}else{Self::base_dir().join(p)}}
 pub fn snapshot(&self)->serde_json::Value{serde_json::json!({"PrinterName":self.printer_name,"PrinterDisplayName":self.printer_display_name,"TemplatePath":self.template_path,"PrintMode":if self.print_mode=="printer"{"pdfprint"}else{&self.print_mode},"PdfOutputPath":self.pdf_output_path,"PdfPrinterExe":self.pdf_printer_exe,"PollSeconds":self.poll_seconds,"MaxPdfPages":self.max_pdf_pages,"AutoUpdateEnabled":self.auto_update_enabled})}
}
#[derive(Debug,Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Card {#[serde(default)]pub name:String,#[serde(default)]pub color:String,#[serde(default)]pub quantity:String,#[serde(default)]pub is_final:bool}
#[derive(Debug,Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase")]
pub struct Job {
 pub job_id:String,#[serde(default="one")]pub copies:usize,#[serde(default)]pub cards:Vec<Card>,
 #[serde(default="one")]pub start_number:usize,#[serde(default)]pub cells:BTreeMap<String,Option<String>>
}
fn one()->usize{1}
impl Job{
 pub fn parse(s:&str)->Result<Self,String>{let j:Self=serde_json::from_str(s).map_err(|e|e.to_string())?;j.check()?;Ok(j)}
 pub fn check(&self)->Result<(),String>{
  if self.job_id.trim().is_empty()||self.job_id.len()>128||!(1..=100).contains(&self.copies)||self.cards.len()>800||self.cells.len()>100||!(1..=1000000).contains(&self.start_number){return Err("JobId, copies, cards, cells hoặc startNumber không hợp lệ".into())}
  for (k,v) in &self.cells{let addr=k.rsplit('!').next().unwrap_or(k).trim_start_matches('$').replace('$',"");if addr.is_empty()||addr.len()>12||!addr.chars().all(|c|c.is_ascii_alphanumeric())||v.as_ref().is_some_and(|s|s.len()>1000){return Err(format!("Ô không hợp lệ: {k}"));}}
  Ok(())
 }
 pub fn part(&self,offset:usize,count:usize,copy:usize)->Self{let mut j=self.clone();j.job_id=format!("{}_part_{}_copy_{}",self.job_id,offset/count.max(1)+1,copy+1);j.copies=1;j.start_number+=offset;j.cards=self.cards.iter().skip(offset).take(count).cloned().collect();j}
}
pub fn agent_id()->Result<String,String>{let p=Config::base_dir().join("agent-id.txt");if p.exists(){let s=fs::read_to_string(&p).map_err(|e|e.to_string())?;let t=s.trim();if uuid::Uuid::parse_str(t).is_ok(){return Ok(t.into())}return Err("agent-id.txt không hợp lệ; không tự thay mã thiết bị".into())}let id=uuid::Uuid::new_v4().to_string();fs::write(&p,&id).map_err(|e|e.to_string())?;Ok(id)}
pub fn log(message:&str){
 use chrono::{FixedOffset,Utc};
 use std::io::Write;
 // Ghi thời gian Nhật Bản (UTC+09:00) nhất quán, kể cả khi máy Windows đổi múi giờ.
 let jst=FixedOffset::east_opt(9*3600).expect("JST offset hợp lệ");
 let timestamp=Utc::now().with_timezone(&jst).format("%Y-%m-%d %H:%M:%S%.3f JST").to_string();
 let line=format!("[{timestamp}] {message}\n");
 let p=Config::base_dir().join("print-agent-rust.log");
 if let Ok(mut f)=fs::OpenOptions::new().create(true).append(true).open(p){let _=f.write_all(line.as_bytes());}
 println!("[{timestamp}] {message}");
}
#[cfg(test)]mod tests{
use super::*;
#[test]fn parse_source_sample(){let j=Job::parse(include_str!("../../sample-job.json")).unwrap();assert!(j.cards.len()>0);}
#[test]fn default_settings(){assert!(Config::default().check().is_ok());}
}
