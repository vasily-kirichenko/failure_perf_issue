#![feature(iterator_find_map)]

extern crate hyper;
extern crate tokio_core;
extern crate futures;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;

use tokio_core::reactor::Core;
use hyper::{Uri, Client, Method};
use hyper::header::Accept;
use futures::{Future, Stream};
use std::*;
use std::cmp::Ordering;
use hyper::Request;
use failure::Error;
use std::path::Path;
use hyper::header::*;

fn download_string(core: &mut Core, uri: Uri) -> Result<String, Error> {
    let mut req = Request::new(Method::Get, uri);
    req.headers_mut().set(Accept::json());
    let client = Client::new(&core.handle());

    let work = client.request(req).and_then(|res| { res.body().concat2() });

    let response = core.run(work)?;
    let response_text = str::from_utf8(&response)?;
    println!("Resp: {}", response_text);
    Ok(response_text.to_string())
}

#[derive(Debug, Fail)]
enum AppError {
    #[fail(display = "No content disposition in response")]
    NoContentDispositionInResponse,
    #[fail(display = "No attachment")]
    NoAttachment,
}

fn option_to_result<T, E>(opt: Option<T>, f: impl FnOnce() -> E) -> Result<T, E> {
    match opt {
        Some(x) => Ok(x),
        None => Err(f())
    }
}

fn download(core: &mut Core, uri: Uri, out_dir: &Path) -> Result<String, Error> {
    let client = Client::new(&core.handle());
    let work = client.get(uri).map(|res| -> Result<Vec<u8>, Error> {
        let content_type = res.headers().get::<ContentType>();
        let disp = res.headers().get::<ContentDisposition>();

        let content_disposition = option_to_result(res.headers().get::<ContentDisposition>(), || AppError::NoContentDispositionInResponse)?;
        let buff =
            match content_disposition.disposition {
                DispositionType::Attachment => {
                    println!("params: {:?}", content_disposition.parameters);
                    option_to_result(
                        content_disposition.parameters
                            .iter()
                            .find_map(|x| match x {
                                DispositionParam::Filename(_, _, buff) => Some(buff),
                                _ => None
                            }),
                        || AppError::NoAttachment)
                }
                _ => Err(AppError::NoAttachment)
            }?;
        Ok(buff.to_vec())
    });
    let buff = core.run(work)??;
    let file_name = str::from_utf8(&buff).map(|x| x.to_string())?;
    Ok(file_name)
}

#[derive(Deserialize, Debug)]
struct Builds {
    build: Vec<Build>
}

#[derive(Deserialize, Debug)]
struct Build {
    id: i64,
    number: String,
}

fn main() -> Result<(), Error> {
    let mut core = Core::new()?;
    let root_url = "http://localhost/guestAuth";
    let build_type = "Preparation";
    let branch = "branch".to_string();

    let builds_uri: Uri = format!("{}/app/rest/builds?locator=status:SUCCESS,state:finished,buildType:{},branch:(name:{},default:any)",
                                  root_url, build_type, branch).parse()?;

    let response = download_string(&mut core, builds_uri)?;
    let mut builds: Builds = serde_json::from_str(&response)?;

    builds.build.sort_by(|x, y| {
        x.number.parse::<i64>()
            .and_then(|x| y.number.parse::<i64>().map(|y| y.cmp(&x)))
            .unwrap_or(Ordering::Equal)
    });

    let last_build = builds.build.first().expect(&format!("No successful builds of branch {}", branch));
    let artifact_uri: Uri = format!("{}/repository/downloadAll/{}/{}:id/artifacts.zip", root_url, build_type, last_build.id).parse()?;
    let out_dir = format!("d:/db_artifacts/{}_{}", build_type, last_build.id);
    let out_dir = Path::new(&out_dir);
    fs::create_dir_all(out_dir)?;
    let file_name = download(&mut core, artifact_uri, out_dir)?;
    Ok(())
}
