mod wasi_http;

use anyhow::Result;
use base64::Engine;
use rand::seq::SliceRandom;
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use wasi as bindings;
use wasi_http::http_request;

#[derive(Debug, Default)]
struct Place {
    name: String,
    lat: f64,
    lng: f64,
    place_id: String,
    address: String,
    rating: f64,
    pics_data: Vec<Vec<u8>>,
    pics_alt_texts: Vec<String>,
    mstd_media_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct Geopoint {
    lat: f64,
    lng: f64,
    iso2: String,
    population: Option<i64>,
}

#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error("No City Picked")]
    NoCityPicked,
    #[error("No image from google")]
    NoImageFromGoogle,
    #[error("IoError")]
    IoError(#[from] std::io::Error),
    #[error("Anyhow error")]
    AnyhowError(#[from] anyhow::Error),
}

fn get_random_city(r: &mut Place, g: Vec<Geopoint>) -> Result<(), MyError> {
    let mut weighted_points: Vec<Geopoint> = Vec::new();
    let weighted_countries = vec!["DE", "GB", "FR", "ES", "IT", "TW", "TH", "VN", "MX", "PT", "KR"];

    let mg = g
        .iter()
        .filter(|&g| g.population.unwrap_or(0) > 25000_i64)
        .cloned()
        .collect::<Vec<Geopoint>>();

    for gp in mg {
        weighted_points.push(gp.clone());
        if weighted_countries.contains(&gp.iso2.as_str()) {
            weighted_points.push(gp.clone());
        }
    }

    match weighted_points.choose(&mut rand::thread_rng()) {
        Some(c) => {
            r.lat = c.lat;
            r.lng = c.lng;
        }
        None => return Err(MyError::NoCityPicked),
    }

    Ok(())
}

async fn ask_to_google(r: &Place) -> Result<Vec<Value>> {
    let api_key = env::var("GOOGLE_API_KEY").expect("GOOGLE_API_KEY not set");
    let url = format!(
        "https://maps.googleapis.com/maps/api/place/nearbysearch/json?location={},{}&radius=50000&type=cafe&keyword=coffee&key={}",
        r.lat, r.lng, api_key
    );

    let resp_body = http_request(bindings::http::types::Method::Get, &url, vec![], None).await?;
    let resp: Value = serde_json::from_slice(&resp_body)?;

    let mut filtered_places: Vec<Value> = Vec::new();
    if let Some(results) = resp["results"].as_array() {
        for i in results {
            let types = i["types"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
            if types.iter().any(|t| {
                let s = t.as_str().unwrap_or("");
                matches!(s, "hotel" | "lodge" | "gas_station" | "convenience_store" | "restaurant" | "bar")
            }) {
                continue;
            }
            if i["rating"].as_f64().unwrap_or(0_f64) >= 3_f64
                && i["user_ratings_total"].as_f64().unwrap_or(0_f64) >= 100_f64
            {
                filtered_places.push(i.clone());
            }
        }
    }

    Ok(filtered_places)
}

async fn search_nearby(r: &mut Place) -> Result<()> {
    let mut filtered_places = ask_to_google(r).await?;
    while filtered_places.is_empty() {
        // In WASI, we don't have thread::sleep, but we can use poll/subscribe
        // For simplicity, let's just pick another city immediately
        let geopoints = get_geopoints()?;
        get_random_city(r, geopoints).map_err(|e| anyhow::anyhow!(e))?;
        filtered_places = ask_to_google(r).await?;
    }

    let p = filtered_places
        .choose(&mut rand::thread_rng())
        .expect("getting filtered_places randomly failed");

    r.place_id = p["place_id"].as_str().unwrap_or_default().to_string();
    r.name = p["name"].as_str().unwrap_or_default().to_string();
    r.rating = p["rating"].as_f64().unwrap_or(0.0);
    r.address = p["vicinity"].as_str().unwrap_or_default().to_string();

    Ok(())
}

async fn get_place_details(r: &mut Place) -> Result<(), MyError> {
    let api_key = env::var("GOOGLE_API_KEY").expect("GOOGLE_API_KEY not set");
    let url = format!(
        "https://maps.googleapis.com/maps/api/place/details/json?place_id={}&fields=photos,formatted_address&key={}",
        r.place_id, api_key
    );

    let resp_body = http_request(bindings::http::types::Method::Get, &url, vec![], None)
        .await
        .map_err(|e| MyError::AnyhowError(e))?;
    let resp: Value = serde_json::from_slice(&resp_body).map_err(|e| MyError::AnyhowError(e.into()))?;

    if let Some(addr) = resp["result"]["formatted_address"].as_str() {
        r.address = addr.to_string();
    }

    let mut pic_urls = Vec::new();
    if let Some(photos) = resp["result"]["photos"].as_array() {
        if photos.is_empty() {
            return Err(MyError::NoImageFromGoogle);
        }
        let n = photos.len().min(4);
        for i in 0..n {
            if let Some(photo_ref) = photos[i]["photo_reference"].as_str() {
                pic_urls.push(format!(
                    "https://maps.googleapis.com/maps/api/place/photo?maxwidth=800&photoreference={}&key={}",
                    photo_ref, api_key
                ));
            }
        }
    } else {
        return Err(MyError::NoImageFromGoogle);
    }

    for url in pic_urls {
        let data = http_request(bindings::http::types::Method::Get, &url, vec![], None)
            .await
            .map_err(|e| MyError::AnyhowError(e))?;
        r.pics_data.push(data);
    }

    Ok(())
}

async fn generate_alt_texts(r: &mut Place) -> Result<(), MyError> {
    let gemini_key = env::var("GEMINI_API_KEY").or_else(|_| env::var("GOOGLE_API_KEY"));
    
    let gemini_key = match gemini_key {
        Ok(k) => k,
        Err(_) => {
            println!("Warning: Neither GEMINI_API_KEY nor GOOGLE_API_KEY is set. Skipping alt-text generation.");
            return Ok(());
        }
    };

    let gemini_uri = env::var("GEMINI_API_KEY_API_URI")
        .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1/models/gemini-1.5-flash:generateContent".to_string());
    
    // Some proxies might require the key in the path or as a header, but the standard 
    // Google API uses a query parameter. 
    let url = if gemini_uri.contains("key=") {
        gemini_uri
    } else {
        let separator = if gemini_uri.contains('?') { "&" } else { "?" };
        format!("{}{}{}key={}", gemini_uri, "", separator, gemini_key)
    };

    println!("Using Gemini API at: {}", if url.contains("key=") { url.split("key=").next().unwrap_or("").to_string() + "key=***" } else { url.clone() });

    for (i, data) in r.pics_data.iter().enumerate() {
        println!("Generating alt-text for image {}/{}...", i + 1, r.pics_data.len());
        let base64_image = base64::engine::general_purpose::STANDARD.encode(data);
        
        let prompt = "Describe this image in detail for a Mastodon alt-text description. Focus on the cafe atmosphere, decor, or coffee/food shown. Keep it under 400 characters.";
        
        let body = json!({
            "contents": [{
                "parts": [
                    {"text": prompt},
                    {
                        "inline_data": {
                            "mime_type": "image/jpeg",
                            "data": base64_image
                        }
                    }
                ]
            }]
        });

        let body_bytes = match serde_json::to_vec(&body) {
            Ok(b) => b,
            Err(e) => {
                println!("Warning: Failed to serialize Gemini request: {:?}", e);
                r.pics_alt_texts.push("A cafe image.".to_string());
                continue;
            }
        };

        let headers = vec![
            ("Content-Type".to_string(), "application/json".to_string().into_bytes()),
        ];

        match http_request(bindings::http::types::Method::Post, &url, headers, Some(body_bytes)).await {
            Ok(resp_body) => {
                match serde_json::from_slice::<Value>(&resp_body) {
                    Ok(resp) => {
                        let alt_text = resp["candidates"][0]["content"]["parts"][0]["text"]
                            .as_str()
                            .unwrap_or("A cafe image.")
                            .trim()
                            .to_string();
                        
                        r.pics_alt_texts.push(alt_text);
                    },
                    Err(e) => {
                        println!("Warning: Failed to parse Gemini response for image {}: {:?}", i + 1, e);
                        r.pics_alt_texts.push("A cafe image.".to_string());
                    }
                }
            },
            Err(e) => {
                println!("Warning: Failed to generate alt-text for image {}: {:?}", i + 1, e);
                r.pics_alt_texts.push("A cafe image.".to_string());
            }
        }
    }
    Ok(())
}

async fn upload_mstd_images(r: &mut Place) -> Result<(), MyError> {
    let access_token = env::var("MSTDN_ACCESS_TOKEN").expect("MSTDN_ACCESS_TOKEN not set");
    let mstdn_uri = env::var("MSTDN_URI").expect("MSTDN_URI not set");

    for (i, data) in r.pics_data.iter().enumerate() {
        let url = format!("https://{}/api/v2/media", mstdn_uri);
        let boundary = "---------------------------12345678901234567890";
        let alt_text = r.pics_alt_texts.get(i).cloned().unwrap_or_else(|| "A cafe image.".to_string());
        
        let mut body = Vec::new();
        // File part
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(format!("Content-Disposition: form-data; name=\"file\"; filename=\"img-{}.jpg\"\r\n", i).as_bytes());
        body.extend_from_slice(b"Content-Type: image/jpeg\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");

        // Description part
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"description\"\r\n\r\n");
        body.extend_from_slice(alt_text.as_bytes());
        body.extend_from_slice(b"\r\n");

        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        let headers = vec![
            ("Authorization".to_string(), format!("Bearer {}", access_token).into_bytes()),
            ("Content-Type".to_string(), format!("multipart/form-data; boundary={}", boundary).into_bytes()),
        ];

        let resp_body = http_request(bindings::http::types::Method::Post, &url, headers, Some(body))
            .await
            .map_err(|e| MyError::AnyhowError(e))?;
        
        let it: Value = serde_json::from_slice(&resp_body).map_err(|e| MyError::AnyhowError(e.into()))?;
        if let Some(id) = it["id"].as_str() {
            r.mstd_media_ids.push(id.to_string());
        }
    }
    Ok(())
}

async fn post_message(r: &Place) -> Result<(), MyError> {
    let mstdn_uri = env::var("MSTDN_URI").expect("MSTDN_URI not set");
    let access_token = env::var("MSTDN_ACCESS_TOKEN").expect("MSTDN_ACCESS_TOKEN not set");

    let msg = format!(
        "{}\n{}\n{}\nhttps://www.google.com/maps/search/?api=1&query={},{}&query_place_id={}\n#coffee #cafe",
        r.name,
        r.address,
        rating_stars(r.rating).unwrap_or_default(),
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

    let body = serde_json::to_vec(&b).map_err(|e| MyError::AnyhowError(e.into()))?;
    let headers = vec![
        ("Authorization".to_string(), format!("Bearer {}", access_token).into_bytes()),
        ("Content-Type".to_string(), "application/json".to_string().into_bytes()),
    ];

    http_request(
        bindings::http::types::Method::Post,
        &format!("https://{}/api/v1/statuses", mstdn_uri),
        headers,
        Some(body),
    )
    .await
    .map_err(|e| MyError::AnyhowError(e))?;

    println!("New msg posted");
    Ok(())
}

fn rating_stars(rating: f64) -> Option<String> {
    let major = rating.floor() as usize;
    let minor = rating % 1.0;
    let mut star = "★".repeat(major);
    if minor > 0.0 {
        star.push('☆');
    }
    Some(star)
}

fn get_geopoints() -> Result<Vec<Geopoint>> {
    let pointscsv = include_str!("geopoints.csv").as_bytes();
    let mut geopoints: Vec<Geopoint> = Vec::new();
    let mut rdr = csv::Reader::from_reader(pointscsv);
    for result in rdr.deserialize() {
        let record: Geopoint = result?;
        geopoints.push(record);
    }
    Ok(geopoints)
}

fn main() -> Result<()> {
    println!("Start checking");

    futures::executor::block_on(async {
        if let Err(e) = run().await {
            eprintln!("Error: {:?}", e);
        }
    });

    println!("Done");
    Ok(())
}

async fn run() -> Result<()> {
    let geopoints = get_geopoints()?;

    let mut rr: Place = Place::default();
    get_random_city(&mut rr, geopoints).map_err(|e| anyhow::anyhow!(e))?;
    search_nearby(&mut rr).await?;
    
    println!("name: {}", rr.name);
    println!("pid: {}", rr.place_id);
    println!("rating: {}", rr.rating);
    
    get_place_details(&mut rr).await.map_err(|e| anyhow::anyhow!(e))?;
    println!("address: {}", rr.address);
    
    generate_alt_texts(&mut rr).await.map_err(|e| anyhow::anyhow!(e))?;
    
    upload_mstd_images(&mut rr).await.map_err(|e| anyhow::anyhow!(e))?;
    post_message(&rr).await.map_err(|e| anyhow::anyhow!(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_random_city() {
        let geopoints = get_geopoints().unwrap();
        let mut rr: Place = Place::default();
        let c = get_random_city(&mut rr, geopoints);
        assert!(c.is_ok());
        assert!(rr.lat != 0.0);
        assert!(rr.lng != 0.0);
    }

    #[test]
    fn test_rating_stars() {
        assert_eq!(rating_stars(4.0).unwrap(), "★★★★");
        assert_eq!(rating_stars(4.2).unwrap(), "★★★★☆");
        assert_eq!(rating_stars(3.7).unwrap(), "★★★☆");
    }
}
