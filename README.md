# HTTP Redirector

A simple http redirection service with access logging based on an input key-link table.

> hr -c "./example-redirect" -p 80

The input table is fetched depending on the param `--config` or `-c`, if it is a url (begin with `http`) then via http/https, if it is a path then from local file, see `example-redirect` for an example. Note that in the table the `https://` prefix of all links in the table should be omited, while `http://` prefix should be reserved.

The request method must be GET or HEAD, otherwise returns 400. If the key in the table is matched, returns 307 with the corresponding link, otherwise returns 404.

All access expect to `/` will be logged in the specified file as JSON if the param `--log-path` or `-l` is provided as a file path, or log will be written to stdout.
