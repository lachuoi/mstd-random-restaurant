// Copyright 2026 Seungjin Kim
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use anyhow::Result;
use wasi as bindings;

pub async fn http_request(
    method: bindings::http::types::Method,
    url: &str,
    headers: Vec<(String, Vec<u8>)>,
    body: Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    http_request_recursive(method, url, headers, body, 0).await
}

#[async_recursion::async_recursion]
async fn http_request_recursive(
    method: bindings::http::types::Method,
    url: &str,
    headers: Vec<(String, Vec<u8>)>,
    body: Option<Vec<u8>>,
    redirect_count: u32,
) -> Result<Vec<u8>> {
    if redirect_count > 5 {
        return Err(anyhow::anyhow!("too many redirects"));
    }

    use bindings::http::outgoing_handler::handle;
    use bindings::http::types::{
        Fields, OutgoingBody, OutgoingRequest, Scheme,
    };

    let parsed_url = url::Url::parse(url)?;
    let scheme = match parsed_url.scheme() {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        _ => Scheme::Other(parsed_url.scheme().to_string()),
    };

    let request_headers = Fields::new();
    let mut has_content_length = false;
    for (k, v) in &headers {
        if k.eq_ignore_ascii_case("content-length") {
            has_content_length = true;
        }
        request_headers
            .set(k, &[v.clone()])
            .map_err(|_| anyhow::anyhow!("failed to set header {}", k))?;
    }
    if !has_content_length {
        if let Some(ref b) = body {
            request_headers
                .set(
                    &"Content-Length".to_string(),
                    &[b.len().to_string().into_bytes()],
                )
                .map_err(|_| {
                    anyhow::anyhow!("failed to set Content-Length header")
                })?;
        }
    }

    let path = parsed_url.path();
    let query = parsed_url.query();
    let path_with_query = if let Some(q) = query {
        format!("{}?{}", path, q)
    } else {
        path.to_string()
    };

    let request = OutgoingRequest::new(request_headers);
    let authority = parsed_url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("missing host in URL: {}", url))?;
    request
        .set_method(&method)
        .map_err(|_| anyhow::anyhow!("failed to set method"))?;
    request
        .set_scheme(Some(&scheme))
        .map_err(|_| anyhow::anyhow!("failed to set scheme"))?;
    request
        .set_authority(Some(authority))
        .map_err(|_| anyhow::anyhow!("failed to set authority"))?;
    request
        .set_path_with_query(Some(&path_with_query))
        .map_err(|_| anyhow::anyhow!("failed to set path"))?;

    // Get the body and its write stream BEFORE calling handle,
    // because handle consumes the request.
    let outgoing_body = request
        .body()
        .map_err(|_| anyhow::anyhow!("failed to get body"))?;
    let stream = outgoing_body
        .write()
        .map_err(|_| anyhow::anyhow!("failed to get stream"))?;

    let future_response = handle(request, None)
        .map_err(|e| anyhow::anyhow!("failed to send request: {:?}", e))?;

    if let Some(ref b) = body {
        for chunk in b.chunks(4096) {
            stream
                .blocking_write_and_flush(chunk)
                .map_err(|_| anyhow::anyhow!("failed to write body chunk"))?;
        }
    }

    // We must drop the stream and finish the body before the response will be sent/received.
    drop(stream);
    OutgoingBody::finish(outgoing_body, None)
        .map_err(|_| anyhow::anyhow!("failed to finish body"))?;

    // Poll for the response
    let pollable = future_response.subscribe();
    loop {
        if let Some(result) = future_response.get() {
            let response = result
                .map_err(|_| anyhow::anyhow!("request failed"))?
                .map_err(|_| anyhow::anyhow!("HTTP error"))?;

            let status = response.status();

            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                let fields = response.headers();
                let location_vec = fields.get(&"Location".to_string());
                if !location_vec.is_empty() {
                    if let Some(location_bytes) = location_vec.get(0) {
                        let location =
                            String::from_utf8_lossy(location_bytes).to_string();
                        // Resolve relative URLs
                        let absolute_location = if location.starts_with("http")
                        {
                            location
                        } else {
                            parsed_url.join(&location)?.to_string()
                        };

                        // For 303, always change method to GET. For 301/302, most clients do too.
                        let is_get = matches!(
                            method,
                            bindings::http::types::Method::Get
                        );
                        let next_method = if status == 303
                            || (matches!(status, 301 | 302) && !is_get)
                        {
                            bindings::http::types::Method::Get
                        } else {
                            method.clone()
                        };

                        // When the method changes to GET, the request body and
                        // its Content-* headers must be dropped.
                        let (headers, body) = if !is_get
                            && matches!(
                                next_method,
                                bindings::http::types::Method::Get
                            ) {
                            let headers = headers
                                .into_iter()
                                .filter(|(k, _)| {
                                    !k.eq_ignore_ascii_case("content-length")
                                        && !k.eq_ignore_ascii_case(
                                            "content-type",
                                        )
                                })
                                .collect();
                            (headers, None)
                        } else {
                            (headers, body)
                        };

                        return http_request_recursive(
                            next_method,
                            &absolute_location,
                            headers,
                            body,
                            redirect_count + 1,
                        )
                        .await;
                    }
                }
            }

            if status < 200 || status >= 300 {
                let mut error_bytes: Vec<u8> = Vec::new();
                if let Ok(body) = response.consume() {
                    if let Ok(stream) = body.stream() {
                        loop {
                            match stream.blocking_read(1024 * 64) {
                                Ok(data) => {
                                    if data.is_empty() {
                                        break;
                                    }
                                    error_bytes.extend_from_slice(&data);
                                }
                                Err(
                                    bindings::io::streams::StreamError::Closed,
                                ) => break,
                                Err(_) => break,
                            }
                        }
                    }
                }

                let error_body = String::from_utf8_lossy(&error_bytes);

                eprintln!("\n=== HTTP REQUEST FAILED ===");
                eprintln!("Method: {:?}", method);
                eprintln!("URL: {}", url);
                eprintln!("Status Code: {}", status);
                eprintln!("Response Body: {}", error_body);
                eprintln!("===========================\n");

                return Err(anyhow::anyhow!(
                    "HTTP request to {} failed with status {}: {}",
                    url,
                    status,
                    error_body
                ));
            }

            let body = response
                .consume()
                .map_err(|_| anyhow::anyhow!("failed to consume response"))?;
            let stream = body.stream().map_err(|_| {
                anyhow::anyhow!("failed to get response stream")
            })?;

            let mut buf = Vec::new();
            loop {
                let chunk = stream.blocking_read(1024 * 64);
                match chunk {
                    Ok(data) => {
                        if data.is_empty() {
                            break;
                        }
                        buf.extend_from_slice(&data);
                    }
                    Err(bindings::io::streams::StreamError::Closed) => break,
                    Err(e) => {
                        return Err(anyhow::anyhow!("stream error: {:?}", e));
                    }
                }
            }
            return Ok(buf);
        }
        pollable.block();
    }
}
