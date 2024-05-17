# Rcurl
Rcurl is a implementation of the curl library  that provides a set of functions for making HTTP requests.

You can use it to make HTTP requests like curl to a server and get the response.

# Document
```
Usage: rcurl.exe [OPTIONS] <URL>

Arguments:
  <URL>  The request url,like http://www.google.com

Options:
  -X, --method-option <HTTP Method>                        The http method,like GET,POST,etc
  -d, --body-option <BODY_OPTION>                          The body of the http request
  -H, --headers <HEADERS>                                  The http headers
  -c, --certificate-path-option <CERTIFICATE_PATH_OPTION>  The pem path
  -A, --user-agent-option <USER_AGENT_OPTION>              The User Agent
  -b, --cookie-option <COOKIE_OPTION>                      The Cookie option
  -o, --file-path-option <FILE_PATH_OPTION>                The downloading file path
  -k, --skip-certificate-validate                          Skip certificate validation
  -r, --range-option <RANGE_OPTION>                        Range validation
  -v, --debug                                              The debug switch
  -h, --help                                               Print help
  -V, --version                                            Print version
```
# HTTP/HTTPS
## GET
```
rcurl http://httpbin.org/get
rcurl https://httpbin.org/get
```
## POST with body
```
 rcurl -X POST -d '{"a":1,"b":2}'  http://httpbin.org/post
```
## PUT
```
 rcurl -X PUT http://httpbin.org/put
```
## DEBUG
```
rcurl http://httpbin.org/get -v
```
## Skip Certificate validate  
```
rcurl http://httpbin.org/get -k
```
## Download file
```
rcurl -o test.html http://httpbin.org/get
```
## Headers
```
rcurl -H 'a:b' -H 'c:d' http://httpbin.org/get
```

## User Agent
```
rcurl -A 'a:b' http://httpbin.org/get
```
## Cookie
```
rcurl -b 'a:b' http://httpbin.org/get
```