# HTTP Redirector

A simple http redirection service with access logging based on an input key-link table.

> hr -p 80 -c "https://cdn.jsdelivr.net/gh/stackinspector/http-redirector/example-redirect" -l "path/to/log/folder/"

The input table is fetched according to the URL via http/https request (if begin with `http`) or local file, see `example-redirect` for an example. Note that the `https://` prefix of all links in the table should be omited, while `http://` prefix should be reserved.

The request method must be GET or HEAD, otherwise returns 400. If the key in the table is matched, returns 307 with the corresponding link, otherwise returns 404.

Access to the matched key will be logged in the specified file as JSON.
