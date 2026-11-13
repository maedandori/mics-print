
use rsfbclient::prelude::*;
use serde_json::{json,Value};
use sha2::{Sha256,Digest};
use std::{fs,thread,time::Duration};

#[derive(Default)]
struct Config { enabled:bool,host:String,port:u16,database:String,user:String,url:String,seconds:u64 }
fn load()->Result<Config,String>{
 let path=crate::production::Config::base_dir().join("mics.ini");
 if !path.exists(){return Ok(Config::default())}
 let s=fs::read_to_string(path).map_err(|e|e.to_string())?;
 let mut c=Config{port:3050,seconds:30,..Config::default()};
 let mut inside=false;
 for line in s.lines(){
  let line=line.trim();
  if line.is_empty()||line.starts_with('#')||line.starts_with(';'){continue}
  if line.starts_with('['){inside=line.eq_ignore_ascii_case("[mics]");continue}
  if !inside{continue}
  if let Some((k,v))=line.split_once('='){
   let v=v.trim();
   match k.trim().to_lowercase().as_str(){
    "enabled"=>c.enabled=v=="1"||v.eq_ignore_ascii_case("true"),
    "host"=>c.host=v.into(),
    "port"=>c.port=v.parse().map_err(|_|"Invalid port")?,
    "database"=>c.database=v.into(),
    "user"=>c.user=v.into(),
    "sync_url"=>c.url=v.into(),
    "interval_seconds"=>c.seconds=v.parse().map_err(|_|"Invalid interval")?,
    _=>{}
   }
  }
 }
 if c.enabled && (c.host.is_empty()||c.database.is_empty()||c.user.is_empty()||!c.url.starts_with("https://")||!(15..=3600).contains(&c.seconds)){
  return Err("Invalid MICS configuration; require host/database/user/https URL and interval 15..3600".into())
 }
 Ok(c)
}
fn read(c:&Config,pass:&str)->Result<Value,String>{
 let mut b=rsfbclient::builder_pure_rust();
 b.host(&c.host).port(c.port).db_name(&c.database).user(&c.user).pass(pass);
 b.with_transaction(|tr|tr.read_only().no_wait());
 let mut db=b.connect().map_err(|e|e.to_string())?;
 // Only reference the planned completion field if SCHEDULE exposes it.
 let mend_rows:Vec<(i64,)>=db.query(
  "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='SCHEDULE' AND TRIM(RDB$FIELD_NAME)='MEND'",()
 ).map_err(|e|e.to_string())?;
 let has_mend=mend_rows.first().is_some_and(|row|row.0>0);
 let mend_expr=if has_mend {"COALESCE(TRIM(CAST(S.MEND AS VARCHAR(40))), '')"} else {"CAST('' AS VARCHAR(40))"};
 // REALPICKQTY is the actual per-cycle robot pick count. Fail safely if absent on older MICS schemas.
 let pick_rows:Vec<(i64,)>=db.query(
  "SELECT COUNT(*) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='MACHINEWORK' AND TRIM(RDB$FIELD_NAME)='REALPICKQTY'",()
 ).map_err(|e|e.to_string())?;
 let pick_expr=if pick_rows.first().is_some_and(|row|row.0>0) {
  "COALESCE(TRIM(CAST(W.REALPICKQTY AS VARCHAR(24))), '')"
 } else {"CAST('' AS VARCHAR(24))"};
 let sql=format!(
 "SELECT TRIM(CAST(W.MACHINENO AS VARCHAR(24))), \
 COALESCE(TRIM(CAST(W.MORDERNO AS VARCHAR(40))), ''), \
 COALESCE(TRIM(CAST(W.STATUS AS VARCHAR(16))), ''), \
 COALESCE(TRIM(CAST(S.PARTSNAME AS VARCHAR(255))), ''), \
 COALESCE(TRIM(CAST(T.TOOLNO AS VARCHAR(80))), ''), \
 COALESCE(TRIM(CAST(T.TOOLNAME AS VARCHAR(255))), ''), \
 COALESCE(TRIM(CAST(W.TOTALQTYOK AS VARCHAR(24))), ''), \
 COALESCE(TRIM(CAST(W.TOTALQTYNG AS VARCHAR(24))), ''), \
 COALESCE(TRIM(CAST(W.STAFFNAME AS VARCHAR(100))), ''), \
 COALESCE(TRIM(CAST(W.QTYPLAN AS VARCHAR(24))), ''), \
 COALESCE(TRIM(CAST(W.REALCYCLETIME AS VARCHAR(24))), ''), \
 {}, \
 {} \
 FROM MACHINEWORK W \
 LEFT JOIN SCHEDULE S ON S.MORDERNO=W.MORDERNO AND S.MACHINENO=W.MACHINENO AND COALESCE(S.DELETEFLG,0)=0 \
 LEFT JOIN TOOL T ON T.ENTRYNO=S.TOOLENTRYNO \
 WHERE W.MACHINENO IN (5,6,7,8,10,11,12,13,14,15,16,17,18,19) \
 ORDER BY W.MACHINENO",mend_expr,pick_expr);
 let rows:Vec<(String,String,String,String,String,String,String,String,String,String,String,String,String)>=db.query(&sql,()).map_err(|e|e.to_string())?;
 let mut result:std::collections::BTreeMap<i32,Value>=std::collections::BTreeMap::new();
 let qty=|s:&str|s.trim().parse::<i64>().unwrap_or(0).max(0);
 for (machine,order,status,product,tool_no,mold,good,bad,staff,plan,cycle,scheduled_end,pick) in rows{
  let Ok(no)=machine.parse::<i32>() else{continue};
  if let Some(existing)=result.get_mut(&no){
   if existing["order_no"]!=order||existing["mold_name"]!=mold {
    existing["status_code"]=json!("AMBIGUOUS");
    existing["mold_name"]=json!("");
    existing["staff_name"]=json!("");
   }
   continue;
  }
  let p=qty(&plan);let ok=qty(&good);
  result.insert(no,json!({"machine_no":no,"order_no":order,"status_code":status,
   "product_name":product,"mold_no":tool_no,"mold_name":mold,
   "qty_plan":p,"qty_ok":ok,"qty_ng":qty(&bad),"qty_remaining":(p-ok).max(0),
   "cycle_seconds":cycle.parse::<f64>().ok(),"pick_qty":pick.trim().parse::<f64>().ok().filter(|v|v.is_finite()&&*v>0.0),"staff_name":staff.trim(),
   "scheduled_end_at":if scheduled_end.trim().is_empty(){None}else{Some(scheduled_end.trim())}}));
 }
 Ok(json!({"source":"mics-data","machines":result.into_values().collect::<Vec<_>>()}))
}

pub fn test_sync(host:&str, port:&str, database:&str, user:&str, url:&str, password:&str, token:&str)->Result<String,String>{
 let port=port.parse::<u16>().map_err(|_|"Firebird port không hợp lệ".to_string())?;
 if host.trim().is_empty()||database.trim().is_empty()||user.trim().is_empty()||password.is_empty()||token.is_empty(){
  return Err("Thiếu cấu hình Firebird hoặc credentials MICS".into())
 }
 let parsed=url::Url::parse(url.trim()).map_err(|e|format!("Sync URL không hợp lệ: {e}"))?;
 if parsed.scheme()!="https"||parsed.host_str().is_none()||parsed.username()!=""||parsed.password().is_some()||parsed.fragment().is_some(){
  return Err("Sync URL phải là HTTPS hợp lệ".into())
 }
 let c=Config{enabled:true,host:host.into(),port,database:database.into(),user:user.into(),url:url.into(),seconds:30};
 let lease_config=crate::production::lease::config()?;
 let permission=if lease_config.enabled {
    Some(crate::production::lease::acquire(&lease_config,token)?
      .ok_or("Test sync blocked: primary has no coordinator lease")?)
 } else {None};
 let mut data=read(&c,password).map_err(|e|format!("FIREBIRD_READ_FAILED: {e}"))?;
 let machines=data["machines"].as_array().ok_or("Không tìm thấy machines")?;
 let count=machines.len();
 if count==0{return Err("Firebird kết nối được nhưng không trả về máy nào; không gửi bản ghi rỗng".into())}
 let with_staff=machines.iter().filter(|x|x["staff_name"].as_str().is_some_and(|v|!v.trim().is_empty())).count();
 data["sampled_at"]=json!(chrono::Utc::now().to_rfc3339());
 let response=if let Some(original)=permission{
    let now=crate::production::lease::acquire(&lease_config,token)?
      .ok_or("Test sync lost coordinator lease")?;
    if now.epoch!=original.epoch{return Err("Test sync coordinator epoch changed".into())}
    let endpoint=format!("{}/sync",lease_config.url.trim_end_matches('/'));
    let result=crate::production::lease::post(&endpoint,token,&lease_config,&now,data)?;
    return Ok(format!("Firebird test sync accepted by fenced Coordinator, {count} machines; result={}",result["ok"]))
 }else{
    ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build()
      .post(url.trim()).set("Authorization",&format!("Bearer {token}")).send_json(data)
      .map_err(|e|format!("WORKER_SYNC_FAILED: {e}"))?
 };
 if !(200..300).contains(&response.status()){return Err(format!("WORKER_HTTP_{}",response.status()))}
 let status=response.status();
 let body:Value=response.into_json().unwrap_or(Value::Null);
 if body.get("success").and_then(Value::as_bool)==Some(false)||body.get("ok").and_then(Value::as_bool)==Some(false){
  return Err(format!("Worker trả về lỗi: {}",body))
 }
 Ok(format!("Firebird: OK, {count} máy; STAFFNAME có dữ liệu: {with_staff}/{count}. Worker HTTP {status} đã nhận request. Chưa xác minh riêng từng cột D1: cần kiểm tra Worker và đọc lại D1."))
}

pub fn start_background(){
 thread::spawn(||{
  let c=match load(){Ok(v)=>v,Err(e)=>{crate::production::log(&format!("MICS_CONFIG_ERROR {e}"));return}};
  if !c.enabled{return}
  let credentials=match crate::credentials::load(){
   Ok(v)=>v,
   Err(e)=>{crate::production::log(&format!("MICS_VAULT_ERROR {e}"));return}
  };
  let pass=credentials.db_password;
  let token=credentials.mics_sync_token;
  let mut last=String::new();
  let lease_config=match crate::production::lease::config(){Ok(v)=>v,Err(e)=>{crate::production::log(&format!("MICS_LEASE_CONFIG_ERROR {e}"));return}};
  let mut last_presence:Option<std::time::Instant>=None;
  loop{
   if lease_config.enabled && last_presence.map(|t|t.elapsed()>=Duration::from_secs(60)).unwrap_or(true){
     if let Err(e)=crate::production::lease::presence(&lease_config,&token){
       crate::production::log(&format!("MICS_PRIMARY_PRESENCE_ERROR {e}"));
     }
     last_presence=Some(std::time::Instant::now());
   }
   let permission=if lease_config.enabled{
     match crate::production::lease::acquire(&lease_config,&token){
       Ok(Some(grant))=>Some(grant),
       Ok(None)=>{crate::production::log("MICS_PRIMARY_STANDBY");thread::sleep(Duration::from_secs(c.seconds));continue},
       Err(e)=>{crate::production::log(&format!("MICS_LEASE_ERROR {e}"));thread::sleep(Duration::from_secs(c.seconds));continue}
     }
   }else{None};
   match read(&c,&pass){
    Ok(mut snap)=>{
     let hash=format!("{:x}",Sha256::digest(serde_json::to_vec(&snap).unwrap_or_default()));
     if hash!=last {
      snap["sampled_at"]=json!(chrono::Utc::now().to_rfc3339());
      if lease_config.enabled{
       // Re-check after Firebird read; no upload after lease loss.
       match crate::production::lease::acquire(&lease_config,&token){
        Ok(Some(next)) if permission.as_ref().is_some_and(|v|v.epoch==next.epoch)=>{
         let endpoint=format!("{}/sync",lease_config.url.trim_end_matches('/'));
         match crate::production::lease::post(&endpoint,&token,&lease_config,&next,snap){
          Ok(_)=>{last=hash;crate::production::log("MICS_PRIMARY_SYNC_OK")},
          Err(e)=>crate::production::log(&format!("MICS_PRIMARY_SYNC_ERROR {e}"))
         }
        },
        _=>crate::production::log("MICS_PRIMARY_LEASE_LOST")
       }
      }else{
       match ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build()
        .post(&c.url).set("Authorization",&format!("Bearer {token}")).send_json(snap){
        Ok(response) if response.status()==200=>{last=hash;crate::production::log("MICS_SYNC_OK");},
        Ok(response)=>crate::production::log(&format!("MICS_SYNC_HTTP {}",response.status())),
        Err(e)=>crate::production::log(&format!("MICS_SYNC_FAILED {e}"))
       }
      }
     }
    }
    Err(e)=>{
     crate::production::log(&format!("MICS_READ_FAILED {e}"));
     // Yield the Firebird lease on persistent DB read failure. Back off
     // instead of immediately grabbing it again and starving the backup.
     if let Some(g)=permission.as_ref(){
       let _=crate::production::lease::release(&lease_config,&token,g);
       thread::sleep(Duration::from_secs(90));
     }
    }
   }
   thread::sleep(Duration::from_secs(c.seconds));
  }
 });
}
#[cfg(test)]
mod tests{#[test]fn disabled_by_default(){assert!(!super::Config::default().enabled)}}
