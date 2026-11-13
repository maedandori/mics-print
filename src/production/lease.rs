//! Optional primary lease: fail closed when coordinator is enabled.
use serde::Deserialize;
use std::{fs,path::PathBuf,time::Duration};
#[derive(Deserialize)]
pub struct Grant {
    pub granted:bool,
    pub epoch:u64,
    pub lease_token:String,
    pub expires_at:String,
}
#[derive(Clone)]
pub struct LeaseConfig{
    pub enabled:bool,
    pub url:String,
    pub id:String,
    pub display_name:String,
}
pub fn config()->Result<LeaseConfig,String>{
    let path=crate::production::Config::base_dir().join("mics.ini");
    let raw=fs::read_to_string(path).unwrap_or_default();
    let mut inside=false;
    let mut enabled=false;
    let mut url=String::new();
    let mut id=String::new();
    let mut display_name=String::new();
    for line in raw.lines(){
        let line=line.trim();
        if line.starts_with('['){inside=line.eq_ignore_ascii_case("[mics]");continue}
        if !inside||line.starts_with('#')||line.starts_with(';'){continue}
        if let Some((k,v))=line.split_once('='){
            match k.trim().to_ascii_lowercase().as_str(){
                "failover_enabled"=>enabled=v.trim()=="1",
                "coordinator_url"=>url=v.trim().to_owned(),
                "display_name"=>display_name=v.trim().to_owned(),
                _=>{}
            }
        }
    }
    if enabled {
        id=format!("primary-{}",crate::production::agent_id()?);
        let parsed=url::Url::parse(&url).map_err(|e|e.to_string())?;
        if parsed.scheme()!="https"||parsed.host_str().is_none()||parsed.query().is_some()||parsed.fragment().is_some(){
            return Err("Coordinator must be HTTPS".into())
        }
        if id.len()<3||id.len()>100||!id.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'_'||c==b'-'){
            return Err("Invalid primary agent ID".into())
        }
    }
    if display_name.is_empty(){
        display_name=std::env::var("COMPUTERNAME").unwrap_or_else(|_|"MICS-PRINT".into());
    }
    if display_name.chars().any(|c|c.is_control()){return Err("Invalid PC display name".into())}
    display_name=display_name.chars().take(60).collect();
    Ok(LeaseConfig{enabled,url,id,display_name})
}
pub fn presence(cfg:&LeaseConfig,secret:&str)->Result<(),String>{
    if !cfg.enabled{return Ok(())}
    if secret.is_empty(){return Err("MICS token missing for presence".into())}
    let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build()
        .post(&format!("{}/presence",cfg.url.trim_end_matches('/')))
        .set("Authorization",&format!("Bearer {secret}"))
        .send_json(serde_json::json!({"role":"primary","agent_id":cfg.id,
            "display_name":cfg.display_name,"standby_enabled":true}))
        .map_err(|e|format!("PRIMARY_PRESENCE_FAILED {e}"))?;
    if response.status()!=200{return Err(format!("PRIMARY_PRESENCE_HTTP {}",response.status()))}
    Ok(())
}
pub fn acquire(cfg:&LeaseConfig,secret:&str)->Result<Option<Grant>,String>{
    if !cfg.enabled{return Ok(None)}
    if secret.is_empty(){return Err("MICS token missing for coordinator".into())}
    let reply=ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()
        .post(&format!("{}/lease",cfg.url.trim_end_matches('/')))
        .set("Authorization",&format!("Bearer {secret}"))
        .send_json(serde_json::json!({"role":"primary","agent_id":cfg.id,"ttl_seconds":45}))
        .map_err(|e|format!("PRIMARY_LEASE_REQUEST_FAILED {e}"))?;
    let grant:Grant=reply.into_json().map_err(|e|e.to_string())?;
    if !grant.granted{return Ok(None)}
    if grant.epoch==0||grant.lease_token.len()<16{return Err("Invalid primary lease".into())}
    Ok(Some(grant))
}
pub fn post(url:&str,secret:&str,config:&LeaseConfig,grant:&Grant,payload:serde_json::Value)->Result<serde_json::Value,String>{
    let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build()
        .post(url).set("Authorization",&format!("Bearer {secret}"))
        .set("X-Mics-Agent-Id",&config.id)
        .set("X-Mics-Lease-Token",&grant.lease_token)
        .set("X-Mics-Lease-Epoch",&grant.epoch.to_string())
        .send_json(payload).map_err(|e|format!("LEASE_UPLOAD_FAILED {e}"))?;
    let result:serde_json::Value=response.into_json().map_err(|e|e.to_string())?;
    if result["ok"]!=true{return Err("Coordinator did not acknowledge upload".into())}
    Ok(result)
}

pub fn release(config:&LeaseConfig,secret:&str,grant:&Grant)->Result<(),String>{
    let endpoint=format!("{}/release",config.url.trim_end_matches('/'));
    let response=ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build()
        .post(&endpoint).set("Authorization",&format!("Bearer {secret}"))
        .set("X-Mics-Agent-Id",&config.id)
        .set("X-Mics-Lease-Token",&grant.lease_token)
        .set("X-Mics-Lease-Epoch",&grant.epoch.to_string())
        .send_json(serde_json::json!({}))
        .map_err(|e|format!("Lease release failed: {e}"))?;
    let data:serde_json::Value=response.into_json().map_err(|e|e.to_string())?;
    if data["ok"]!=true{return Err("Coordinator did not acknowledge release".into())}
    Ok(())
}
