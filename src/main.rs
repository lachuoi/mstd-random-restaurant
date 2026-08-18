// Copyright 2026 Seungjin Kim
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

mod wasi_http;

use anyhow::Result;
use base64::Engine;
use rand::seq::SliceRandom;
use serde::Deserialize;
use serde_json::{Value, json};
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
    photo_references: Vec<String>,
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
    let weighted_countries_env = env::var("WEIGHTED_COUNTRIES")
        .unwrap_or_else(|_| "DE,GB,FR,ES,IT,TW,TH,VN,MX,PT,KR".to_string());
    let weighted_countries: Vec<&str> =
        weighted_countries_env.split(',').collect();

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
    let excluded_types_env =
        env::var("EXCLUDED_PLACE_TYPES").unwrap_or_else(|_| {
            "hotel,lodge,lodging,gas_station,convenience_store,\
             grocery_or_supermarket,night_club,bar,cafe"
                .to_string()
        });
    let excluded_types: Vec<&str> =
        excluded_types_env.split(',').map(str::trim).collect();

    let url = format!(
        "https://maps.googleapis.com/maps/api/place/nearbysearch/json?location={},{}&radius=50000&type=restaurant&key={}",
        r.lat, r.lng, api_key
    );

    let resp_body =
        http_request(bindings::http::types::Method::Get, &url, vec![], None)
            .await?;
    let resp: Value = serde_json::from_slice(&resp_body)?;

    let mut filtered_places: Vec<Value> = Vec::new();
    if let Some(results) = resp["results"].as_array() {
        for i in results {
            let types =
                i["types"].as_array().map(|v| v.as_slice()).unwrap_or(&[]);
            if types
                .iter()
                .any(|t| excluded_types.contains(&t.as_str().unwrap_or("")))
            {
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

    if let Some(photos) = p["photos"].as_array() {
        for photo in photos {
            if let Some(photo_ref) = photo["photo_reference"].as_str() {
                r.photo_references.push(photo_ref.to_string());
            }
        }
    }

    Ok(())
}

async fn get_place_details(r: &mut Place) -> Result<(), MyError> {
    let api_key = env::var("GOOGLE_API_KEY").expect("GOOGLE_API_KEY not set");

    // Always fetch details to get as many photos as possible and the formatted_address.
    let url = format!(
        "https://maps.googleapis.com/maps/api/place/details/json?place_id={}&fields=photos,formatted_address&key={}",
        r.place_id, api_key
    );

    let resp_body =
        http_request(bindings::http::types::Method::Get, &url, vec![], None)
            .await
            .map_err(|e| MyError::AnyhowError(e))?;
    let resp: Value = serde_json::from_slice(&resp_body)
        .map_err(|e| MyError::AnyhowError(e.into()))?;

    if let Some(addr) = resp["result"]["formatted_address"].as_str() {
        r.address = addr.to_string();
    }

    // Clear existing photo references and get all from details
    r.photo_references.clear();
    if let Some(photos) = resp["result"]["photos"].as_array() {
        for photo in photos {
            if let Some(photo_ref) = photo["photo_reference"].as_str() {
                r.photo_references.push(photo_ref.to_string());
            }
        }
    }

    if r.photo_references.is_empty() {
        return Err(MyError::NoImageFromGoogle);
    }

    // Limit to 4 photos for Mastodon
    let n = r.photo_references.len().min(4);
    for i in 0..n {
        let photo_ref = &r.photo_references[i];
        let url = format!(
            "https://maps.googleapis.com/maps/api/place/photo?maxwidth=800&photoreference={}&key={}",
            photo_ref, api_key
        );
        let data = http_request(
            bindings::http::types::Method::Get,
            &url,
            vec![],
            None,
        )
        .await
        .map_err(|e| MyError::AnyhowError(e))?;
        println!("Downloaded photo {}/{} ({} bytes)", i + 1, n, data.len());
        r.pics_data.push(data);
    }

    Ok(())
}

async fn generate_alt_texts(r: &mut Place) -> Result<(), MyError> {
    if r.pics_data.is_empty() {
        return Ok(());
    }

    let gemini_key =
        env::var("GEMINI_API_KEY").or_else(|_| env::var("GOOGLE_API_KEY"));
    let gemini_key = match gemini_key {
        Ok(k) => k,
        Err(_) => {
            println!(
                "Warning: Neither GEMINI_API_KEY nor GOOGLE_API_KEY is set. Skipping alt-text generation."
            );
            return Ok(());
        }
    };

    let gemini_uri = env::var("GEMINI_API_KEY_API_URI")
        .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1/models/gemini-1.5-flash:generateContent".to_string());

    let url: String = if gemini_uri.contains("key=") {
        gemini_uri
    } else {
        let separator = if gemini_uri.contains('?') { "&" } else { "?" };
        format!("{}{}{}key={}", gemini_uri, "", separator, gemini_key)
    };

    println!(
        "Generating alt-texts for {} images one by one with 1s buffer...",
        r.pics_data.len()
    );

    for (i, data) in r.pics_data.iter().enumerate() {
        if i > 0 {
            // 1 second buffer
            let duration = 1_000_000_000; // 1 second in nanoseconds
            bindings::clocks::monotonic_clock::subscribe_duration(duration)
                .block();
        }

        let base64_image =
            base64::engine::general_purpose::STANDARD.encode(data);

        let body = json!({
            "contents": [{
                "parts": [
                    {"text": "Describe this image for Mastodon alt-text. Focus on the restaurant atmosphere, decor, or food shown. Keep the description under 400 characters."},
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
                println!(
                    "Warning: Failed to serialize Gemini request for image {}: {:?}",
                    i, e
                );
                r.pics_alt_texts.push("A restaurant image.".to_string());
                continue;
            }
        };

        let headers = vec![(
            "Content-Type".to_string(),
            "application/json".to_string().into_bytes(),
        )];

        match http_request(
            bindings::http::types::Method::Post,
            &url,
            headers,
            Some(body_bytes),
        )
        .await
        {
            Ok(resp_body) => {
                match serde_json::from_slice::<Value>(&resp_body) {
                    Ok(resp) => {
                        let text = resp["candidates"][0]["content"]["parts"][0]
                            ["text"]
                            .as_str()
                            .unwrap_or("A restaurant image.")
                            .trim();
                        r.pics_alt_texts.push(text.to_string());
                    }
                    Err(e) => {
                        println!(
                            "Warning: Failed to parse Gemini response for image {}: {:?}",
                            i, e
                        );
                        r.pics_alt_texts
                            .push("A restaurant image.".to_string());
                    }
                }
            }
            Err(e) => {
                println!(
                    "Warning: Failed to generate alt-text for image {}: {:?}",
                    i, e
                );
                r.pics_alt_texts.push("A restaurant image.".to_string());
            }
        }
    }

    // Ensure we have at least one alt-text for each image
    while r.pics_alt_texts.len() < r.pics_data.len() {
        r.pics_alt_texts.push("A restaurant image.".to_string());
    }

    Ok(())
}

async fn upload_mstd_images(r: &mut Place) -> Result<(), MyError> {
    let access_token =
        env::var("MSTDN_ACCESS_TOKEN").expect("MSTDN_ACCESS_TOKEN not set");
    let mstdn_uri = env::var("MSTDN_URI").expect("MSTDN_URI not set");

    for (i, data) in r.pics_data.iter().enumerate() {
        println!(
            "Uploading image {}/{} ({} bytes) to Mastodon...",
            i + 1,
            r.pics_data.len(),
            data.len()
        );
        let url = format!("https://{}/api/v2/media", mstdn_uri);
        let boundary = "---------------------------12345678901234567890";
        let alt_text = r
            .pics_alt_texts
            .get(i)
            .cloned()
            .unwrap_or_else(|| "A restaurant image.".to_string());

        let mut body = Vec::new();
        // File part
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(format!("Content-Disposition: form-data; name=\"file\"; filename=\"img-{}.jpg\"\r\n", i).as_bytes());
        body.extend_from_slice(b"Content-Type: image/jpeg\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");

        // Description part
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"description\"\r\n\r\n",
        );
        body.extend_from_slice(alt_text.as_bytes());
        body.extend_from_slice(b"\r\n");

        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        let headers = vec![
            (
                "Authorization".to_string(),
                format!("Bearer {}", access_token).into_bytes(),
            ),
            (
                "Content-Type".to_string(),
                format!("multipart/form-data; boundary={}", boundary)
                    .into_bytes(),
            ),
        ];

        let resp_body = http_request(
            bindings::http::types::Method::Post,
            &url,
            headers,
            Some(body),
        )
        .await
        .map_err(|e| MyError::AnyhowError(e))?;

        let it: Value = serde_json::from_slice(&resp_body)
            .map_err(|e| MyError::AnyhowError(e.into()))?;
        if let Some(id) = it["id"].as_str() {
            r.mstd_media_ids.push(id.to_string());
        }
    }
    Ok(())
}

async fn post_message(r: &Place) -> Result<(), MyError> {
    println!("Posting status message to Mastodon...");
    let mstdn_uri = env::var("MSTDN_URI").expect("MSTDN_URI not set");
    let access_token =
        env::var("MSTDN_ACCESS_TOKEN").expect("MSTDN_ACCESS_TOKEN not set");

    let msg = format!(
        "{}\n{}\n{}\nhttps://www.google.com/maps/search/?api=1&query={},{}&query_place_id={}\n#food #restaurant",
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

    let body =
        serde_json::to_vec(&b).map_err(|e| MyError::AnyhowError(e.into()))?;
    let headers = vec![
        (
            "Authorization".to_string(),
            format!("Bearer {}", access_token).into_bytes(),
        ),
        (
            "Content-Type".to_string(),
            "application/json".to_string().into_bytes(),
        ),
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

    for i in 0..10 {
        let mut rr: Place = Place::default();
        if let Err(e) = get_random_city(&mut rr, geopoints.clone()) {
            eprintln!("Attempt {}: Error picking city: {:?}", i + 1, e);
            continue;
        }

        if let Err(e) = search_nearby(&mut rr).await {
            eprintln!("Attempt {}: Error searching nearby: {:?}", i + 1, e);
            continue;
        }

        println!(
            "Attempt {}: Checking restaurant: {} ({})",
            i + 1,
            rr.name,
            rr.place_id
        );

        match get_place_details(&mut rr).await {
            Ok(_) => {
                if rr.pics_data.len() < 4 {
                    println!(
                        "Only {} images found, trying another...",
                        rr.pics_data.len()
                    );
                    continue;
                }

                println!("Found restaurant with {} images", rr.pics_data.len());
                println!("rating: {}", rr.rating);
                println!("address: {}", rr.address);

                generate_alt_texts(&mut rr)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                upload_mstd_images(&mut rr)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                post_message(&rr).await.map_err(|e| anyhow::anyhow!(e))?;
                return Ok(());
            }
            Err(e) => {
                eprintln!("Attempt {}: Error getting details: {:?}", i + 1, e);
                continue;
            }
        }
    }

    Err(anyhow::anyhow!(
        "Could not find a restaurant with 4 images after 10 attempts"
    ))
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
