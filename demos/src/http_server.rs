use super::{debug, error, info, trace};
use embedded_nal::nb::block;
use embedded_nal::TcpFullStack;

use super::SocketAddrWrap;
fn usize_to_decimal_string<'a>(value: usize, buffer: &'a mut [u8]) -> &'a str {
    if buffer.len() < 20 {
        return ""; // Return empty string if buffer is too small
    }

    let mut temp = value;
    let mut digits = [0u8; 20];
    let mut len = 0;

    if temp == 0 {
        buffer[0] = b'0';
        return core::str::from_utf8(&buffer[0..1]).unwrap();
    }

    while temp > 0 {
        digits[len] = (temp % 10) as u8 + b'0';
        temp /= 10;
        len += 1;
    }

    for i in 0..len {
        buffer[i] = digits[len - 1 - i];
    }
    core::str::from_utf8(&buffer[0..len]).unwrap()
}

#[allow(dead_code)]
#[derive(Debug)]
struct WrapError(httparse::Error);

#[cfg(feature = "defmt")]
impl defmt::Format for WrapError {
    fn format(&self, fmt: defmt::Formatter) {
        match self.0 {
            httparse::Error::HeaderName => defmt::write!(fmt, "HeaderName"),
            httparse::Error::HeaderValue => defmt::write!(fmt, "HeaderValue"),
            httparse::Error::NewLine => defmt::write!(fmt, "NewLine"),
            httparse::Error::Status => defmt::write!(fmt, "Status"),
            httparse::Error::Token => defmt::write!(fmt, "Token"),
            httparse::Error::TooManyHeaders => defmt::write!(fmt, "TooManyHeaders"),
            httparse::Error::Version => defmt::write!(fmt, "Version"),
        }
    }
}

type Handler<'h> = &'h mut dyn FnMut(&[u8], &mut [u8]) -> Result<usize, u16>;

pub struct Path<'paths, 'handler> {
    pub paths: &'paths [&'paths str],
    pub handler: Handler<'handler>,
    pub is_json: bool,
}

const EMBED_INDEX: &[u8] = include_bytes!("static/index.html");
pub const INDEX_PATHS: [&str; 3] = ["/", "index.htm", "index.html"];
pub const LED_PATHS: [&str; 1] = ["/api/led/"];

pub fn embed_index(output: &mut [u8]) -> Result<usize, u16> {
    output[..EMBED_INDEX.len()].copy_from_slice(EMBED_INDEX);
    Ok(EMBED_INDEX.len())
}

pub fn http_server<T, S>(stack: &mut T, port: u16) -> Result<(), T::Error>
where
    T: TcpFullStack<TcpSocket = S> + ?Sized,
{
    let mut led_state = false;
    let mut send_index =
        |_body: &[u8], output: &mut [u8]| -> Result<usize, u16> { embed_index(output) };
    let mut handle_led = |body: &[u8], output: &mut [u8]| -> Result<usize, u16> {
        if !body.is_empty() && body.contains(&b':') {
            led_state = body.windows(4).any(|w| w == b"true");
        }
        let response = if led_state {
            b"{\"led\": true }"
        } else {
            b"{\"led\": false}"
        };
        output[..response.len()].copy_from_slice(response);
        Ok(response.len())
    };

    let mut known_paths = [
        Path {
            paths: INDEX_PATHS.as_slice(),
            handler: &mut send_index,
            is_json: false,
        },
        Path {
            paths: LED_PATHS.as_slice(),
            handler: &mut handle_led,
            is_json: true,
        },
    ];

    http_server_args(stack, port, &mut known_paths)
}

pub fn http_server_args<'paths, 'handler, T, S>(
    stack: &mut T,
    port: u16,
    known_paths: &mut [Path<'paths, 'handler>],
) -> Result<(), T::Error>
where
    T: TcpFullStack<TcpSocket = S> + ?Sized,
{
    let mut send_buffer = [0; 2048];
    let mut content_length_buffer = [0; 20];

    let mut sock = stack.socket()?;
    debug!("-----Binding to TCP port {}-----", port);
    stack.bind(&mut sock, port)?;
    info!("-----Bound to TCP port {}-----", port);

    stack.listen(&mut sock)?;
    info!("-----Listening-----");

    loop {
        let (mut client_sock, addr) = block!(stack.accept(&mut sock))?;
        info!(
            "-----Accepted connection from {:?}-----",
            SocketAddrWrap { addr: &addr }
        );

        let mut buf = [0; 1024];
        let received_len = block!(stack.receive(&mut client_sock, &mut buf))?;
        if received_len == 0 {
            continue;
        }
        info!(
            "-----Received {} bytes from {:?}-----",
            received_len,
            SocketAddrWrap { addr: &addr }
        );

        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        match req.parse(&buf[..received_len]) {
            Ok(httparse::Status::Complete(size)) => {
                debug!("-----Request parsed----- {} bytes", size);
                debug!(
                    " method: {:?} path: {:?} version: {:?}",
                    req.method, req.path, req.version
                );
                for header in req.headers {
                    if ["Host", "Content-Length", "Content-Type", "Connection"]
                        .contains(&header.name)
                    {
                        debug!(
                            " header: {:?} {:?}",
                            header.name,
                            core::str::from_utf8(header.value).unwrap_or("(invalid utf-8)")
                        );
                    } else {
                        trace!("-----Ignored header: {:?}-----", header.name);
                    }
                }
                let body_length = received_len - size;
                let body = if body_length > 0 {
                    debug!("-----Request body: {} bytes-----", body_length);
                    &buf[size..received_len]
                } else {
                    &[]
                };
                let request_path = req.path.unwrap_or("(invalid path)");

                let mut handled = false;

                for path in known_paths.iter_mut() {
                    if path.paths.contains(&request_path) {
                        send_buffer.fill(0);
                        match (path.handler)(body, &mut send_buffer) {
                            Ok(len) => {
                                block!(stack.send(
                                    &mut client_sock,
                                    "HTTP/1.1 200 OK\r\nContent-Length: ".as_bytes()
                                ))?;
                                let content_length =
                                    usize_to_decimal_string(len, &mut content_length_buffer);
                                block!(stack.send(&mut client_sock, content_length.as_bytes()))?;

                                if path.is_json {
                                    block!(stack.send(
                                        &mut client_sock,
                                        "\r\nContent-Type: application/json\r\n\r\n".as_bytes()
                                    ))?;
                                } else {
                                    block!(stack.send(
                                        &mut client_sock,
                                        "\r\nContent-Type: text/html\r\n\r\n".as_bytes()
                                    ))?;
                                }
                                block!(stack.send(&mut client_sock, &send_buffer[..len]))?;
                            }
                            Err(e) => {
                                let mut error_code_buf = [0; 20];
                                let error_code =
                                    usize_to_decimal_string(e as usize, &mut error_code_buf);
                                block!(stack.send(&mut client_sock, "HTTP/1.1 ".as_bytes()))?;
                                block!(stack.send(&mut client_sock, error_code.as_bytes()))?;
                                block!(stack.send(&mut client_sock, " Error\r\n".as_bytes()))?;
                                continue;
                            }
                        };

                        handled = true;
                        break;
                    }
                }
                if !handled {
                    block!(stack.send(&mut client_sock, "HTTP/1.1 404 Not found\r\n".as_bytes()))?;
                }
            }
            Err(e) => {
                error!("-----Error parsing request: {:?}-----", WrapError(e));
                continue;
            }
            Ok(httparse::Status::Partial) => {
                error!("-----Request parsed, but not complete-----");
                continue;
            }
        }

        stack.close(client_sock)?;
    }
}
