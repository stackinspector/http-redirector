# HTTP Redirector

A simple http redirection service based on an input key-link table.

> http-redirector -p 80 -c "https://cdn.jsdelivr.net/gh/stackinspector/http-redirector/example-redirect"

The input table is fetched via http/https request, see [example-redirect](https://cdn.jsdelivr.net/gh/stackinspector/http-redirector/example-redirect) for an example. Note that the `https://` prefix of all links in the table should be omited.

The request method must be GET or HEAD, otherwise returns 400. If the key in the table is matched, returns 307 with the corresponding link, otherwise returns 404.
