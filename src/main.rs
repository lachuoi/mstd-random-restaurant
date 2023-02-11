
use std::env;
use std::error::Error;
use std::io::Cursor;

use log::{debug, error, info, trace, warn};
use log4rs;
use serde_yaml;

use rand::seq::SliceRandom;
use rand::distributions::{Alphanumeric, DistString};

use serde_json::Value;
use serde_json::json;

use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;

use serde::Deserialize;

#[derive(Debug, Default)]
struct Restaurant {
    name: String,
    lat: f64,
    lng: f64,
    place_id: String,
    address: String,
    rating: f64,
    pics: Vec<String>,
    pics_tmp_dir: String,
    mstd_media_ids: Vec<i64>,
}

#[derive(Debug, Deserialize, Clone)]
struct Geopoint {
    lat: f64,
    lng: f64,
    iso2: String,
    population: Option<i64>,
}

fn get_random_city(r: &mut Restaurant, g: Vec<Geopoint>) {

    let mut weighted_points: Vec<Geopoint> = Vec::new();
    for gp in g {
        let weighted_countries = vec!["FR","US","ES","IT","JP","TW","TH","VN","MX","PT","KR"];
        if gp.population != None &&
            gp.population.unwrap() > 25000 &&
            weighted_countries.contains(&gp.clone().iso2.as_str())
        {
            weighted_points.push(gp.clone());
        }
    }

    match weighted_points.choose(&mut rand::thread_rng()) {
        Some(c) => { r.lat = c.lat; r.lng = c.lng; },
        None => panic!("No city picked up"),
    }
}

fn search_nearby(r: &mut Restaurant) -> Result<(), Box<dyn Error>> {
    // Search restaurant nearby the city and pick one
    let api_key = env::var("GOOGLE_API_KEY")
        .expect("You must set the GOOGLE_API_KEY environment var!");
    let url: String = format!(
        "https://maps.googleapis.com/maps/api/place/nearbysearch/json?location={},{}&radius=50000&type=restaurant&key={}",
        r.lat, r.lng, api_key
    );
    let resp: Value = reqwest::blocking::get(url)?.json().unwrap();

    let mut filtered_places: Vec<Value> = Vec::new();
    for i in resp["results"].as_array().unwrap() {
        if ! i["types"].as_array().unwrap().contains(&Value::String("hotel".to_string())) ||
            ! i["types"].as_array().unwrap().contains(&Value::String("lodge".to_string())) ||
            ! i["types"].as_array().unwrap().contains(&Value::String("gas_station".to_string())) ||
            ! i["types"].as_array().unwrap().contains(&Value::String("bar".to_string())) ||
            ! i["types"].as_array().unwrap().contains(&Value::String("cafe".to_string()))
        {
            filtered_places.push(i.clone());
        }
    };

    let p = filtered_places.choose(&mut rand::thread_rng()).unwrap();
    r.place_id = p.clone()["place_id"].as_str().unwrap().to_string();
    r.name = p.clone()["name"].as_str().unwrap().to_string();
    if p.get("rating").is_some() {
        r.rating = p["rating"].as_f64().unwrap();
    } else {
        r.rating = 0.0;
    };
    
    Ok(())
}

fn get_place_details(r: &mut Restaurant) -> Result<(), Box<dyn Error>> {
    // Get restaurnat's detailed photos and formatted_address

    let api_key = env::var("GOOGLE_API_KEY")
        .expect("You must set the GOOGLE_API_KEY environment var!");
    let url: String = format!("https://maps.googleapis.com/maps/api/place/details/json?place_id={}&fields=photos,formatted_address&key={}",
        r.place_id, api_key
    );

    let resp: Value = reqwest::blocking::get(url)?.json().unwrap();

    r.address = resp["result"]["formatted_address"].as_str().unwrap().to_string();
    //println!("{:#?}", resp);

    //println!("{:#?}", resp["result"]["photos"]);

    let mut n: usize = 0;
    if resp["result"]["photos"].as_array().unwrap().len() == 0 {
        info!("No picture from google");
        // return certain error and take care that error at main
    } else if resp["result"]["photos"].as_array().unwrap().len() < 4 {
        n = resp["result"]["photos"].as_array().unwrap().len();
    } else {
        n = 4
    }
    for i in 0..n {
        r.pics.push(
            format!("https://maps.googleapis.com/maps/api/place/photo?maxwidth=640&photoreference={}&key=",
                    resp["result"]["photos"][i]["photo_reference"]
                        .clone().as_str().unwrap().to_string(),
            )
        );
    }

    Ok(())

}

fn verify_nearby(r: &mut Restaurant) -> Result<(), Box<dyn Error>> {
    // Check distance not too far?
    // Nothing to verify now
    Ok(())
}

fn search_street_image(r: &mut Restaurant) -> Result<(), Box<dyn Error>> {
    // First picture is from street image
    let api_key = env::var("GOOGLE_API_KEY")
        .expect("You must set the GOOGLE_API_KEY environment var!");
    let url = format!("https://maps.googleapis.com/maps/api/streetview/metadata?size=640x640&location={},{}&key={}",
        r.lat,
        r.lng,
        api_key,
    );
    let resp: Value = reqwest::blocking::get(url)?.json().unwrap();
    if resp["status"] != "ZERO_RESULT" {
        let pic_url = format!("https://maps.googleapis.com/maps/api/streetview?size=640x640&return_error_codes=true&location={},{}&key=",
                              r.lat,
                              r.lng,
        );
        r.pics.push(pic_url);
    }
    Ok(())
}

async fn get_images(r: &mut Restaurant) -> Result<(), Box<dyn Error>> {
    let temp_dir = format!("{}/{}",
                           env::temp_dir().to_str().unwrap(),
                           Alphanumeric.sample_string(&mut rand::thread_rng(), 8));
    std::fs::create_dir(temp_dir.clone())?;
    r.pics_tmp_dir = temp_dir.clone();

    for (i, url) in r.pics.iter().enumerate()
    {
        let api_key = env::var("GOOGLE_API_KEY")
            .expect("You must set the GOOGLE_API_KEY environment var!");
        let url: String = format!("{url}{api_key}");
        let response = reqwest::get(url).await?;
        let mut file = std::fs::File::create(format!("{temp_dir}/img{i}.jpg"))?;
        let mut content =  Cursor::new(response.bytes().await?);
        std::io::copy(&mut content, &mut file)?;
    }
    //debug!("{:#?}", r.pics);
    Ok(())

}

async fn upload_mstd_images(r: &mut Restaurant) -> Result<(), Box<dyn Error>> {
    let access_token = env::var("MSTDN_ACCESS_TOKEN")
        .expect("You must set the MSTDN_ACCESS_TOKEN environment var!");
    let mstdn_uri: String = env::var("MSTDN_URI")
        .expect("You must set the MSTDN environment var!");

    for (i, _) in r.pics.iter().enumerate() {
        let url = format!("https://{mstdn_uri}/api/v2/media");
        let file = std::fs::read(format!("{}/img{}.jpg", r.pics_tmp_dir, i)).unwrap();
        let file_part = reqwest::multipart::Part::bytes(file)
            .file_name(format!("img-{i}.jpg"))
            .mime_str("image/jpg")
            .unwrap();
        let form = reqwest::multipart::Form::new().part("file", file_part);
        let client = reqwest::Client::new();
        //match client
        let c = client
            .post(url)
            .header(
                AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .multipart(form).send().await?;
        if c.status() != 200 {
            info!("Uploading image failed: {}", c.status());
            panic!("Uploading image failed");
        }
        let it = c.json::<serde_json::Value>().await?;
        r.mstd_media_ids.push(it["id"].as_str().unwrap().parse::<i64>().unwrap());
    }
    Ok(())
}

async fn post_message(r: &Restaurant) -> Result<(), Box<dyn Error>> {
    let mstdn_uri: String = env::var("MSTDN_URI")
        .expect("You must set the MSTDN environment var!");

    let access_token = env::var("MSTDN_ACCESS_TOKEN")
        .expect("You must set the MSTDN_ACCESS_TOKEN environment var!");

    let msg: String = format!("{}\n{}\n{}\nhttps://www.google.com/maps/search/?api=1&query={},{}&query_place_id={}",
        r.name,
        r.address,
        rating_stars(r.rating),
        r.lat,
        r.lng,
        r.place_id,
    );

    let b = json!({
        "status": msg,
        "visibility": "public",
        "language": "eng",
        "media_ids": r.mstd_media_ids,
    });

    let res = reqwest::Client::new()
        .post(format!("https://{mstdn_uri}/api/v1/statuses"))
        .header(
            AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header(
            CONTENT_TYPE,
            "application/json"
        )
        .json(&b).send().await;
    match res {
        Ok(_) => info!("New cartoon posted!"),
        Err(e) => error!("Error on carton posting! {}", e),
    }
    Ok(())
}

fn clean_images(r: &Restaurant) -> std::io::Result<()> {
    std::fs::remove_dir_all(r.pics_tmp_dir.as_str())?;
    Ok(())
}

fn rating_stars(rating: f64) -> String {
    let major: usize = (rating - (rating % 1.0)) as usize;
    let minor: f64 = (rating % 1.0) ;
    let mut star: String = "★".repeat(major);
    if minor > 0.0 {
        star = format!("{star}☆");
    }
    star
}

fn main() -> Result<(), Box<dyn Error>> {
    let config_str = include_str!("log4rs.yaml");
    let config = serde_yaml::from_str(config_str).unwrap();
    log4rs::init_raw_config(config).unwrap();

    let pointscsv = include_str!("geopoints.csv").as_bytes();
    let mut geopoints: Vec<Geopoint> = Vec::new();
    let mut rdr = csv::Reader::from_reader(pointscsv);
    for result in rdr.deserialize() {
        let record: Geopoint = result?;
        geopoints.push(record);
    }

    let mut rr: Restaurant = Restaurant::default();
    get_random_city(&mut rr, geopoints);
    search_nearby(&mut rr);
    info!("name: {}", rr.name);
    info!("pid: {}", rr.place_id);
    info!("rating: {}", rr.rating);
    get_place_details(&mut rr);
    info!("address: {}", rr.address);
    verify_nearby(&mut rr);
    //search_street_image(&mut rr);
    //info!("street_img: {}", rr.pics[0].to_string());

    let runtime = tokio::runtime::Runtime::new().unwrap();
    match runtime.block_on(get_images(&mut rr)) {
        Ok(_) => info!("Image downloaded"),
        Err(_) => error!("Image download failed"),
    };
    match runtime.block_on(upload_mstd_images(&mut rr)) {
        Ok(_) => info!("Image uploaded"),
        Err(_) => error!("Image upload failed"),
    };
    match runtime.block_on(post_message(&rr)) {
        Ok(_) => info!("New msg posted"),
        Err(_) => error!("Posting failed"),
    };

    post_message(&rr);
    clean_images(&rr);

    println!("{:#?}", rr);
    debug!("Hello, world!");

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::{println as info, println as warn, println as debug};
    use std::borrow::Borrow;
    use std::ops::Deref;

    fn logon() {
        let config_str = include_str!("log4rs.yaml");
        let config = serde_yaml::from_str(config_str).unwrap();
        log4rs::init_raw_config(config).unwrap();
    }

    #[test]
    #[ignore = "not yet implemented"]
    fn test_get_random_city() {
        let mut rr: Restaurant = Restaurant::default();
        let c = get_random_city(&mut rr);
        debug!("{:#?}", c);
        //assert!(!c.is_err());
    }

    #[test]
    fn test_search_nearby() {
        let mut rr: Restaurant = Restaurant::default();
        let c = get_random_city(&mut rr);
        //println!("{:#?}", search_nearby(c));
    }

    #[test]
    fn test_rating_stars() {
        println!("{}", rating_stars(3.7));
    }

}






