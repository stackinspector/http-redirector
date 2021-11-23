# HTTP Redirector

A simple http redirection service with access logging based on an input key-link table.

> hr -p 80 -c "https://cdn.jsdelivr.net/gh/stackinspector/http-redirector/example-redirect" -l "path/to/log/folder/"

The input table is fetched via http/https request, see [example-redirect](https://cdn.jsdelivr.net/gh/stackinspector/http-redirector/example-redirect) for an example. Note that the `https://` prefix of all links in the table should be omited.

The request method must be GET or HEAD, otherwise returns 400. If the key in the table is matched, returns 307 with the corresponding link, otherwise returns 404.

Access to the matched key will be recorded in the specified folder as a [sled](https://github.com/spacejam/sled) database.
