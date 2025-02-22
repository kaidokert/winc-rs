use super::{debug, info, trace};
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

fn send_index<'a>(_body: &[u8], output: &'a mut [u8]) -> &'a [u8] {
    let embed_index = include_bytes!("static/index.html");
    // copy the embeded index.html to the output buffer
    output[0..embed_index.len()].copy_from_slice(embed_index);
    return &output[0..embed_index.len()];
}

fn handle_led<'a>(_body: &[u8], output: &'a mut [u8]) -> &'a [u8] {
    let response = "{ \"led\": true }";
    output[0..response.len()].copy_from_slice(response.as_bytes());
    return &output[0..response.len()];
}

type Handler<'a> = fn(&[u8], &'a mut [u8]) -> &'a [u8];

pub fn http_server<T, S>(stack: &mut T, port: u16, loop_forever: bool) -> Result<(), T::Error>
where
    T: TcpFullStack<TcpSocket = S> + ?Sized,
{
    let index_paths = ["/", "index.htm", "index.html"];
    let led_paths = ["/api/led/"];

    let mut sock = stack.socket()?;
    debug!("-----Binding to TCP port {}-----", port);
    stack.bind(&mut sock, port)?;
    info!("-----Bound to TCP port {}-----", port);

    // do listen, accept, and send/receive
    stack.listen(&mut sock)?;
    info!("-----Listening-----");

    loop {
        // In the loop so we can borrow response again every loop
        let known_paths = [
            (index_paths.as_slice(), send_index as Handler<'_>, false),
            (led_paths.as_slice(), handle_led as Handler<'_>, true),
        ];
        let mut content_length_buffer = [0; 20];

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
                    // only dump interesting headers
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
                let path = req.path.unwrap_or("(invalid path)");

                let mut response = [0; 1024];
                let mut handled = false;

                for (paths, handler, is_json) in known_paths {
                    if paths.contains(&path) {
                        let body_slice = handler(body, &mut response);
                        debug!("-----Response body length: {}-----", body_slice.len());

                        let send_header = "HTTP/1.1 200 OK\r\nContent-Length: ";
                        block!(stack.send(&mut client_sock, send_header.as_bytes()))?;
                        let content_length =
                            usize_to_decimal_string(body_slice.len(), &mut content_length_buffer);
                        block!(stack.send(&mut client_sock, content_length.as_bytes()))?;
                        // write content type
                        match is_json {
                            true => block!(stack.send(
                                &mut client_sock,
                                "\r\nContent-Type: application/json\r\n\r\n".as_bytes()
                            ))?,
                            false => block!(stack.send(
                                &mut client_sock,
                                "\r\nContent-Type: text/html\r\n\r\n".as_bytes()
                            ))?,
                        };
                        block!(stack.send(&mut client_sock, body_slice))?;
                        handled = true;
                        break;
                    }
                }
                if !handled {
                    let not_found = "HTTP/1.1 404 File not found\r\n";
                    block!(stack.send(&mut client_sock, not_found.as_bytes()))?;
                }
            }
            Err(e) => {
                debug!("-----Error parsing request: {}-----", e);
                continue;
            }
            Ok(httparse::Status::Partial) => {
                debug!("-----Request parsed, but not complete-----");
                continue;
            }
        }

        stack.close(client_sock)?;
        if !loop_forever {
            info!("Quiting the loop");
            break;
        }
        info!("Looping again");
    }
    Ok(())
}
