# xin
simple sync http client library written in rust

## in cargo.toml
```
xin = { git = "https://github.com/dutchaen/xin" }
```

## example get request with xin
```
use xin::net::{Method, Request};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut request = Request::new(Method::GET, "httpbin.org", 443, "/get")?;
    request.set_header("User-Agent", "xin@dutchaen");
    request.set_body(""); // sets body + finalizes request

    let response = request.perform_with_tls()?;
    
    let headers = response.read_headers(); // HashMap<String, String>
    let text = response.read_body_string(); // String
    let status_code = response.read_status_code(); // u16

    // response.0 <- raw http response as Vec<u8> if you want to use your own parse functions

    println!("Response headers: {:#?}", headers);
    println!("Response text: {}", text);
    println!("Response status code: {}", status_code);
   
    return Ok(());
}
```
