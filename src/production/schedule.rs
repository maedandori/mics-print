// Separate read-only production schedule cache.
// Fetch from company Firebird once per minute, persist locally, sync only when changed.
use rsfbclient::prelude::*;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::{BTreeMap,BTreeSet},fs,path::{Path,PathBuf},thread,time::Duration};
use chrono::{Datelike,FixedOffset,Utc};
const MACHINES:[i32;14]=[5,6,7,8,10,11,12,13,14,15,16,17,18,19];

fn window()->(String,String,String){
 let jst=FixedOffset::east_opt(9*3600).expect("JST");
 let now=Utc::now().with_timezone(&jst);
 // Align to Monday 00:00 JST so the full current and following calendar
 // weeks are available, even when the current day is Friday or Sunday.
 let today=now.date_naive();
 let monday=today-chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
 let start=monday.and_hms_opt(0,0,0).expect("week start");
 let fmt=|d:chrono::NaiveDateTime|d.format("%Y-%m-%d %H:%M:%S").to_string();
 (fmt(start),fmt(start+chrono::Duration::days(14)),fmt(start+chrono::Duration::days(30)))
}
fn normalize_time(s:&str)->Result<String,String>{
 let s=s.trim().replace('T'," ");
 if s.len()<19{return Err("Invalid schedule date".into())}
 let date=s.get(..19).ok_or("Invalid schedule date")?;
 chrono::NaiveDateTime::parse_from_str(date,"%Y-%m-%d %H:%M:%S").map_err(|e|format!("Invalid schedule date: {e}"))?;
 Ok(date.to_string())
}
fn get_config()->Result<Option<(String,u16,String,String,String)>,String>{
 let path=crate::production::Config::base_dir().join("mics.ini");
 if !path.exists(){return Ok(None)}
 let mut active=false;
 let mut map=BTreeMap::new();
 for raw in fs::read_to_string(&path).map_err(|e|e.to_string())?.lines(){
  let line=raw.trim();
  if line.starts_with('['){active=line.eq_ignore_ascii_case("[mics]");continue}
  if !active||line.starts_with([';','#']){continue}
  if let Some((key,value))=line.split_once('='){map.insert(key.trim().to_ascii_lowercase(),value.trim().to_string());}
 }
 let val=|k:&str|map.get(k).map(String::as_str).unwrap_or("");
 if !["1","true"].iter().any(|x|val("enabled").eq_ignore_ascii_case(x)){return Ok(None)}
 let url=url::Url::parse(val("sync_url")).map_err(|e|e.to_string())?;
 if url.scheme()!="https"||url.path()!="/api/integrations/mics/snapshot"||url.query().is_some()||url.fragment().is_some(){
  return Err("Schedule needs the Machine-Control HTTPS snapshot URL".into())
 }
 let endpoint=url.join("/api/integrations/mics/schedule").map_err(|e|e.to_string())?.to_string();
 let port=val("port").parse::<u16>().map_err(|e|e.to_string())?;
 let host=val("host").to_string();
 let db=val("database").to_string();
 let user=val("user").to_string();
 if host.is_empty()||db.is_empty()||user.is_empty(){return Err("Missing Firebird config".into())}
 Ok(Some((host,port,db,user,endpoint)))
}
fn read(host:&str,port:u16,db_name:&str,user:&str,password:&str,first:&str,end:&str)->Result<Vec<Value>,String>{
 let mut b=rsfbclient::builder_pure_rust();
 b.host(host).port(port).db_name(db_name).user(user).pass(password);
 b.with_transaction(|t|t.read_only().no_wait());
 let mut db=b.connect().map_err(|e|e.to_string())?;
 let fields:Vec<(String,)>=db.query(
  "SELECT TRIM(RDB$FIELD_NAME) FROM RDB$RELATION_FIELDS WHERE RDB$RELATION_NAME='SCHEDULE'",()
 ).map_err(|e|e.to_string())?;
 let known:BTreeSet<String>=fields.into_iter().map(|(f,)|f.to_ascii_uppercase()).collect();
 for field in ["MACHINENO","MACHINESEQ","MORDERNO","PARTSNAME","TOOLENTRYNO","MSTART","MEND","DELETEFLG"]{
  if !known.contains(field){return Err(format!("SCHEDULE missing {field}; keeping previous D1 snapshot"))}
 }
 let color=if known.contains("COLORNAME"){"S.COLORNAME"}else if known.contains("COLOR"){"S.COLOR"}else{"NULL"};
 let sql=format!(
  "SELECT TRIM(CAST(S.MACHINENO AS VARCHAR(24))), \
  COALESCE(TRIM(CAST(S.MACHINESEQ AS VARCHAR(30))),''), \
  COALESCE(TRIM(CAST(S.MORDERNO AS VARCHAR(60))),''), \
  COALESCE(TRIM(CAST(S.PARTSNAME AS VARCHAR(255))),''), \
  COALESCE(TRIM(CAST(S.TOOLENTRYNO AS VARCHAR(60))),''), \
  COALESCE(TRIM(CAST(T.TOOLNO AS VARCHAR(100))),''), \
  COALESCE(TRIM(CAST(T.TOOLNAME AS VARCHAR(255))),''), \
  COALESCE(TRIM(CAST({color} AS VARCHAR(100))),''), \
  COALESCE(TRIM(CAST(S.MSTART AS VARCHAR(40))),''), \
  COALESCE(TRIM(CAST(S.MEND AS VARCHAR(40))), '') \
  FROM SCHEDULE S LEFT JOIN TOOL T ON T.ENTRYNO=S.TOOLENTRYNO \
  WHERE COALESCE(S.DELETEFLG,0)=0 AND S.MACHINENO IN (5,6,7,8,10,11,12,13,14,15,16,17,18,19) \
  AND S.MSTART < TIMESTAMP '{end}' AND (S.MEND >= TIMESTAMP '{first}' OR S.MSTART >= TIMESTAMP '{first}') \
  ORDER BY S.MACHINENO,S.MSTART,S.MACHINESEQ,S.MORDERNO");
 let rows:Vec<(String,String,String,String,String,String,String,String,String,String)>=db.query(&sql,()).map_err(|e|e.to_string())?;
 if rows.len()>20000{return Err("Schedule exceeds local cache limit".into())}
 let mut out=Vec::new();
 for (machine,sequence,order,product,entry,mold_no,mold_name,color,start,end) in rows{
  let number=machine.parse::<i32>().map_err(|_|"Invalid machine number")?;
  if !MACHINES.contains(&number){return Err("Unexpected machine number".into())}
  out.push(json!({"machine_no":number,"sequence":sequence,"order_no":order,
   "product_name":product,"tool_entry_no":entry,"mold_no":mold_no,
   "mold_name":mold_name,"color":color,"start_at":normalize_time(&start)?,
   "end_at":if end.is_empty(){Value::Null}else{json!(normalize_time(&end)?)} }));
 }
 Ok(out)
}
fn replace_local(path:&Path,data:&[u8])->Result<(),String>{
 let tmp=path.with_extension("tmp");
 let bak=path.with_extension("bak");
 fs::write(&tmp,data).map_err(|e|e.to_string())?;
 if bak.exists(){fs::remove_file(&bak).map_err(|e|e.to_string())?}
 if path.exists(){fs::rename(path,&bak).map_err(|e|e.to_string())?}
 match fs::rename(&tmp,path){
  Ok(())=>{let _=fs::remove_file(&bak);Ok(())},
  Err(e)=>{if bak.exists(){let _=fs::rename(bak,path);}Err(e.to_string())}
 }
}
pub fn start_background(){
 thread::spawn(||{
  let config=match get_config(){Ok(Some(c))=>c,Ok(None)=>return,Err(e)=>{
   crate::production::log(&format!("MICS_SCHEDULE_CONFIG_ERROR {e}"));return
  }};
  let creds=match crate::credentials::load(){Ok(c)=>c,Err(e)=>{
   crate::production::log(&format!("MICS_SCHEDULE_CREDENTIAL_ERROR {e}"));return
  }};
  if creds.db_password.is_empty()||creds.mics_sync_token.is_empty(){
   crate::production::log("MICS_SCHEDULE_CREDENTIAL_ERROR empty");return
  }
  let dir=crate::production::Config::base_dir().join("mics-schedule-cache");
  if let Err(e)=fs::create_dir_all(&dir){
   crate::production::log(&format!("MICS_SCHEDULE_CACHE_ERROR {e}"));return
  }
  let last_sent=dir.join("last-acknowledged-week.sha256");
  let lease_config=match crate::production::lease::config(){Ok(v)=>v,Err(e)=>{crate::production::log(&format!("SCHEDULE_LEASE_CONFIG_ERROR {e}"));return}};
  loop{
   let run=(||->Result<bool,String>{
    let grant=if lease_config.enabled{
      match crate::production::lease::acquire(&lease_config,&creds.mics_sync_token)?{
       Some(g)=>Some(g),None=>return Ok(false)
      }
    }else{None};
    let (from,to,local_end)=window();
    let rows=match read(&config.0,config.1,&config.2,&config.3,&creds.db_password,&from,&local_end){
      Ok(rows)=>rows,
      Err(e)=>{
        if let Some(g)=grant.as_ref(){
          let _=crate::production::lease::release(&lease_config,&creds.mics_sync_token,g);
        }
        return Err(e)
      }
    };
    let cache=json!({"from":from,"to":local_end,"rows":rows});
    let bytes=serde_json::to_vec(&cache).map_err(|e|e.to_string())?;
    // Write only if the cached content changed.
    let path=dir.join("schedule-local.json");
    if fs::read(&path).ok().as_deref()!=Some(bytes.as_slice()){replace_local(&path,&bytes)?}
    let fortnight:Vec<Value>=rows.into_iter().filter(|r|{
     let start=r["start_at"].as_str().unwrap_or("");
     let end=r["end_at"].as_str().unwrap_or(start);
     start<to.as_str()&&end>=from.as_str()
    }).collect();
    if fortnight.is_empty(){return Err("Schedule returned zero rows; retaining previous D1 data".into())}
    if fortnight.len()>3000{return Err("14-day schedule too large".into())}
    let payload=json!({"source":"mics-print-schedule","window_start":from,"window_end":to,
      "machines":MACHINES,"rows":fortnight});
    let data=serde_json::to_vec(&payload).map_err(|e|e.to_string())?;
    let hash=format!("{:x}",Sha256::digest(&data));
    if fs::read_to_string(&last_sent).ok().as_deref()==Some(hash.as_str()){return Ok(false)}
    if lease_config.enabled{
      // The snapshot is sent only through the fenced coordinator route.
      let next=crate::production::lease::acquire(&lease_config,&creds.mics_sync_token)?
       .ok_or("Schedule lost lease")?;
      if grant.as_ref().is_none_or(|g|g.epoch!=next.epoch){return Err("Schedule epoch changed".into())}
      let endpoint=format!("{}/schedule",lease_config.url.trim_end_matches('/'));
      crate::production::lease::post(&endpoint,&creds.mics_sync_token,&lease_config,&next,payload)?;
    }else{
      let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build()
       .post(&config.4).set("Authorization",&format!("Bearer {}",creds.mics_sync_token))
       .send_json(payload).map_err(|e|format!("Schedule POST: {e}"))?;
      if response.status()!=200{return Err(format!("Schedule HTTP {}",response.status()))}
      let result:Value=response.into_json().map_err(|e|e.to_string())?;
      if result["ok"]!=true{return Err("Schedule Worker did not acknowledge snapshot".into())}
    }
    replace_local(&last_sent,hash.as_bytes())?;
    Ok(true)
   })();
   match run{
    Ok(true)=>crate::production::log("MICS_SCHEDULE_SYNC_OK"),
    Ok(false)=>(),
    Err(e)=>{crate::production::log(&format!("MICS_SCHEDULE_SYNC_ERROR {e}"));if lease_config.enabled{thread::sleep(Duration::from_secs(90));}}
   }
   thread::sleep(Duration::from_secs(60));
  }
 });
}
