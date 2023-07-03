use native_tls::{TlsStream, TlsConnector};
use std::{io::{Read, Write}, collections::HashMap};
use base64::{Engine as _, engine::general_purpose};
use std::net::{TcpStream, SocketAddr, ToSocketAddrs, IpAddr, Ipv4Addr};

pub enum Method {
    GET,
    PUT,
    POST,
    HEAD,
    PATCH,
    DELETE,
    OPTIONS
}

#[derive(Clone)]
pub struct Request {
    raw: Vec<u8>,
    host: String,
    addr: SocketAddr,
    router: TlsConnector,
}

pub struct Response(Vec<u8>);

#[derive(Clone)]
pub struct Proxy {
    addr: SocketAddr,
    authorization_header: String,
}

impl Proxy {
    #[allow(unused_assignments)]
    pub fn parse_http(proxy_str: &str) -> Result<Proxy, Box<dyn std::error::Error>> {
        let mut authorization_header = String::new();
        let mut address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        
        if proxy_str.contains("@") {
            let fragments = proxy_str.split("@").collect::<Vec<_>>();
            let username_password = fragments[0];
            let basic_authorization = general_purpose::STANDARD.encode(username_password);
            authorization_header = format!("Proxy-Authorization: Basic {}\r\n", basic_authorization);
            address = fragments[1].to_socket_addrs()?.collect::<Vec<_>>()[0];
        }
        else {
            address = proxy_str.to_socket_addrs()?.collect::<Vec<_>>()[0];
        }

        return Ok(Proxy{
            addr: address,
            authorization_header,
        });
    }
}

impl Response {
    pub fn read_body(&self) -> Vec<u8> {
        let mut count = 0;
        let mut body_position: usize = 0;
        for (index, rune) in self.0.iter().enumerate() {
            match rune {
                b'\r' | b'\n' => count += 1,
                _ => count = 0,
            };

            if count == 4 {
                body_position = index+1;
                break;
            }
        }

        return self.0[body_position..].to_vec();
    }
    
    pub fn read_body_string(&self) -> String {
        let body_vec = self.read_body();
        return String::from_utf8(body_vec).unwrap_or_default();
    }

    pub fn read_status_code(&self) -> u16 {
        let mut read = false;
        let mut status_text = String::new();
      
        for &rune in self.0.iter() {
            if rune == b' ' {
                match read {
                    true => break,
                    false => {
                        read = true;
                        continue;
                    },
                }
            }

            if read {
                status_text.push(rune as char);
            }
        }
        return status_text.parse::<u16>().unwrap_or(0);
    }

    pub fn read_headers(&self) -> HashMap<String, String> {
        let mut headers: HashMap<String, String> = HashMap::new();

        let request_string = String::from_utf8(self.0.clone()).unwrap_or_default();
        let first_segment = request_string.split("\r\n\r\n").collect::<Vec<_>>()[0];

        for header in first_segment.split("\r\n").skip(1) {
            if let Some(first_colon_pos) = header.find(": ") {
                let key = header[..first_colon_pos].to_string();
                let value = header[first_colon_pos+2..].to_string();
                headers.insert(key, value);
            }
        }
        return headers;
    }
}

impl Request {
    pub fn new(method: Method, host: &str, port: u16, path: &str) -> Result<Request, Box<dyn std::error::Error>> {
        let method_str = match method {
            Method::GET => "GET",
            Method::PUT => "PUT",
            Method::POST => "POST",
            Method::HEAD => "HEAD",
            Method::PATCH => "PATCH",
            Method::DELETE => "DELETE",
            Method::OPTIONS => "OPTIONS"
        };

        let mut raw_bytes: Vec<u8> = Vec::new();
        let first_line = format!("{} {} HTTP/1.1\r\n", method_str, path);
        let host_header = format!("Host: {}\r\n", host);
        let connection_header = format!("Connection: close\r\n");

        raw_bytes.write_all(first_line.as_bytes())?;
        raw_bytes.write_all(host_header.as_bytes())?;
        raw_bytes.write_all(connection_header.as_bytes())?;

        let socket_addrs = format!("{}:{}", host, port).to_socket_addrs()?.collect::<Vec<_>>();

        return Ok(Request {
            raw: raw_bytes,
            host: host.to_string(),
            addr: socket_addrs[0],
            router: TlsConnector::new()?
        });
    }

    pub fn raw_string(&self) -> String {
        return String::from_utf8(self.raw.clone()).unwrap();
    }

    pub fn set_header(&mut self, key: &str, value: &str) {
        let header = format!("{}: {}\r\n", key, value);
        let _ = self.raw.write_all(header.as_bytes());
    }

    pub fn set_body(&mut self, body: &str) {
        let body_segment = format!("\r\n{}", body);
        let _ = self.raw.write_all(body_segment.as_bytes());
    }

    pub fn perform(&self) -> Result<Response, Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(&self.addr)?;
        let _ = stream.write_all(&self.raw)?;
        let raw_response = self.read_all(&mut stream)?;
        return Ok(Response(raw_response));
    }

    pub fn perform_with_tls(&self) -> Result<Response, Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(&self.addr)?;
        let mut tls_stream = self.router.connect(&self.host, stream)?;
        let _ = tls_stream.write_all(&self.raw)?;
        let raw_response = self.read_all_tls(&mut tls_stream)?;
        return Ok(Response(raw_response));
    }

    fn read_all(&self, stream: &mut TcpStream) -> Result<Vec<u8>, std::io::Error> {
        let mut result = Vec::new();
        stream.read_to_end(&mut result)?;
        return Ok(result);
    }

    fn read_all_tls(&self, stream: &mut TlsStream<TcpStream>) -> Result<Vec<u8>, std::io::Error> {
        let mut result = Vec::new();
        stream.read_to_end(&mut result)?;
        return Ok(result);
    }

    pub fn perform_with_http_proxy(&self, proxy: &Proxy) -> Result<Response, Box<dyn std::error::Error>> {
        let mut stream = TcpStream::connect(proxy.addr)?;
        let _ = stream.write_all(&format!("CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n{}\r\n", self.host, self.addr.port(), self.host, self.addr.port(), proxy.authorization_header).as_bytes())?;

        let mut response = String::new();

        loop {
            let mut buffer =  vec![0; 1];
            stream.read_exact(&mut buffer)?;

            response.push(buffer[0] as char);

            if response.ends_with("\r\n\r\n") {
                break;
            }
        }

        if !response.starts_with("HTTP/1.1 200") {
            return Err("proxy connection failure".into());
        }

        stream.write_all(&self.raw)?;
        let raw_response = self.read_all(&mut stream)?;

        stream.shutdown(std::net::Shutdown::Both)?;
        return Ok(Response(raw_response));
    }
}
