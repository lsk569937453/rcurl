# Rcurl

Rcurl is a implementation of the curl library that provides a set of functions for making HTTP/FTP requests.

You can use it to make HTTP/FTP requests like curl to a server and get the response.

# Document

```
Usage: rcurl.exe [OPTIONS] <URL>

Arguments:
  <URL>  The request url,like http://www.google.com

Options:
  -X, --request <method>                                   Specify request method to use
  -d, --data <data>                                        HTTP POST data
  -F, --form <name=content>                                Specify multipart MIME data
  -H, --header <header/@file>                              The http headers
  -c, --certificate-path-option <CERTIFICATE_PATH_OPTION>  The pem path
  -u, --user <user:password>                               Server user and password
  -A, --user-agent <name>                                  Send User-Agent <name> to server
  -b, --cookie <data|filename>                             The Cookie option
  -e, --referer <URL>                                      Referrer URL
  -o, --output <file>                                      Write to file instead of stdout
  -T, --upload-file <file>                                 Transfer local FILE to destination
  -k, --insecure                                           Allow insecure server connections
  -I, --head                                               Show document info only
  -r, --range <range>                                      Retrieve only the bytes within RANGE
  -v, --verbose                                            Make the operation more talkative
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

# FTP/FTPS

## List directory

```
 rcurl -u "demo:password" ftp://test.rebex.net:21/
```

## Upload file

```
rcurl -T LICENSE -u "demo:password" ftp://test.rebex.net:21/
```

# Unit Test Report

```
Coverage Results:
|| Tested/Total Lines:
|| src\cli\app_config.rs: 4/4 +0.00%
|| src\ftp\handler.rs: 38/60 +0.00%
|| src\http\handler.rs: 149/177 +0.00%
|| src\main.rs: 11/17 +0.00%
||
78.29% coverage, 202/258 lines covered, +0.00% change in coverage
```