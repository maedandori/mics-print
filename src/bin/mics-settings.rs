//! Settings for optional read-only Firebird sync. Stored separately from Print Agent.
use std::{fs,io,path::Path};

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct MicsSettings {
 pub enabled:bool,pub host:String,pub port:String,pub database:String,
 pub user:String,pub sync_url:String,pub interval_seconds:String,
}
impl Default for MicsSettings {
 fn default()->Self{Self{enabled:false,host:String::new(),port:"3050".into(),database:String::new(),user:"SYSDBA".into(),sync_url:String::new(),interval_seconds:"30".into()}}
}
impl MicsSettings {
 pub fn load(path:&Path)->Result<Self,String>{
  let raw=match fs::read_to_string(path){
   Ok(s)=>s,
   Err(e) if e.kind()==io::ErrorKind::NotFound=>return Ok(Self::default()),
   Err(e)=>return Err(format!("Không đọc được mics.ini: {e}"))
  };
  let mut c=Self::default();let mut inside=false;
  for line in raw.trim_start_matches('\u{feff}').lines(){
   let line=line.trim();if line.is_empty()||line.starts_with(';')||line.starts_with('#'){continue}
   if line.starts_with('['){inside=line.eq_ignore_ascii_case("[mics]");continue}
   if !inside{continue}
   let Some((key,value))=line.split_once('=') else{continue};
   let value=value.trim().to_string();
   match key.trim().to_ascii_lowercase().as_str(){
    "enabled"=>c.enabled=value=="1"||value.eq_ignore_ascii_case("true"),
    "host"=>c.host=value,"port"=>c.port=value,"database"=>c.database=value,
    "user"=>c.user=value,"sync_url"=>c.sync_url=value,
    "interval_seconds"=>c.interval_seconds=value,
    _=>{}
   }
  }
  Ok(c)
 }
 pub fn check(&self)->Result<(),String>{
  let port=self.port.parse::<u16>().map_err(|_|"Cổng Firebird phải là số")?;
  if port==0{return Err("Cổng Firebird phải từ 1 đến 65535".into())}
  let interval=self.interval_seconds.parse::<u64>().map_err(|_|"Thời gian đồng bộ phải là số nguyên")?;
  if !(15..=3600).contains(&interval){return Err("Thời gian đồng bộ MICS phải từ 15 đến 3600 giây".into())}
  for value in [&self.host,&self.port,&self.database,&self.user,&self.sync_url,&self.interval_seconds]{
   if value.contains(['\r','\n']){return Err("Giá trị không được chứa ký tự xuống dòng".into())}
  }
  if self.enabled{
   if self.host.trim().is_empty()||self.database.trim().is_empty()||self.user.trim().is_empty(){return Err("Nhập đầy đủ host, đường dẫn database và user Firebird".into())}
   let url=url::Url::parse(self.sync_url.trim()).map_err(|_|"Sync URL không hợp lệ")?;
   if url.scheme()!="https"||url.host_str().is_none()||url.username()!=""||url.password().is_some()||url.fragment().is_some(){return Err("Sync URL phải là HTTPS hợp lệ, không chứa tài khoản hoặc fragment".into())}
  }
  Ok(())
 }
 pub fn save(&self,path:&Path)->Result<(),String>{
  self.check()?;
  let previous=match fs::read_to_string(path){
   Ok(s)=>s,
   Err(e) if e.kind()==io::ErrorKind::NotFound=>String::new(),
   Err(e)=>return Err(format!("Không đọc được mics.ini: {e}"))
  };
  let mut output=Vec::<String>::new();let mut in_section=false;let mut found=false;
  // Preserve all unrelated INI sections and unknown keys.
  let keys=["enabled","host","port","database","user","sync_url","interval_seconds"];
  for line in previous.trim_start_matches('\u{feff}').lines(){
   let trimmed=line.trim();
   if trimmed.starts_with('[')&&trimmed.ends_with(']'){
    if in_section{Self::append(&mut output,self);}
    in_section=trimmed.eq_ignore_ascii_case("[mics]");
    if in_section{found=true;}
    output.push(line.to_string());continue;
   }
   if in_section{
    if let Some((key,_))=trimmed.split_once('='){
     if keys.iter().any(|k|key.trim().eq_ignore_ascii_case(k)){continue}
    }
   }
   output.push(line.to_string());
  }
  if in_section{Self::append(&mut output,self);}
  if !found {output.push("[mics]".into());Self::append(&mut output,self);}
  let tmp=path.with_extension("ini.tmp");
  fs::write(&tmp,format!("{}\n",output.join("\n"))).map_err(|e|e.to_string())?;
  let backup=path.with_extension("ini.bak");
  if path.exists(){
   fs::copy(path,&backup).map_err(|e|format!("Không sao lưu mics.ini: {e}"))?;
   if let Err(e)=fs::remove_file(path){let _=fs::remove_file(&tmp);return Err(format!("Không thay mics.ini: {e}"))}
  }
  match fs::rename(&tmp,path) {
   Ok(())=>Ok(()),
   Err(e)=>{if backup.exists(){let _=fs::copy(backup,path);}Err(format!("Không lưu mics.ini: {e}"))}
  }
 }
 fn append(out:&mut Vec<String>,c:&Self){
  for (k,v) in [("enabled",if c.enabled{"1"}else{"0"}),("host",&c.host),("port",&c.port),("database",&c.database),("user",&c.user),("sync_url",&c.sync_url),("interval_seconds",&c.interval_seconds)]{
   out.push(format!("{k}={v}"));
  }
 }
}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn default_off(){assert!(!MicsSettings::default().enabled);}
 #[test] fn validates_interval(){let mut c=MicsSettings::default();c.interval_seconds="14".into();assert!(c.check().is_err());c.interval_seconds="30".into();assert!(c.check().is_ok());}
 #[test] fn rejects_newline(){let mut c=MicsSettings::default();c.host="evil\nsync_url=https://evil".into();assert!(c.check().is_err());}
}
