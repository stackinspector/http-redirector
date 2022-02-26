# HTTP Redirector

A simple http redirection service with access logging based on an input key-link table.

> hr -c "example,./example-redirect" -p 80

Startup parameter `--config` or `-c` determines how the program fetches the input. For URLs beginning with 'http', the program will fetch via HTTP/HTTPS, otherwise, the program will use a local path. Examples are listed in `example-redirect`.

Only GET or HEAD methods are allowed for requests. Other methods will cause an HTTP 400 error. The server will respond with HTTP 307 and the corresponding link when the key is matched, otherwise, the server will respond with an HTTP 404 error. 

Logs recording all accesses except to `/` will be written to stdout by default unless a file path is indicated through parameter `--log-path` or `-l`.
