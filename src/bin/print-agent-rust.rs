#[path = "../production/mod.rs"] mod production;
use std::{env,process::ExitCode};
fn main()->ExitCode{
 let args:Vec<String>=env::args().collect();
 let result=if args.iter().any(|x|x=="--test-pdf"){
  let config=production::Config::load(&production::Config::base_dir().join("config.ini"));
  config.and_then(|c|{let path=production::Config::base_dir().join("sample-job.json");
   let data=std::fs::read_to_string(path).map_err(|e|e.to_string())?;
   let job=production::Job::parse(&data)?;
   production::engine::pdf_output(&c,&job)
  })
 }else{production::engine::run(args.iter().any(|x|x=="--enable-live"))};
 match result {Ok(())=>ExitCode::SUCCESS,Err(e)=>{production::log(&format!("LỖI: {e}"));ExitCode::FAILURE}}
}
