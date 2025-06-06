# Crabflow 🦀

Crabflow is a powerful tool for running REST workflows. It allows you to define and execute sequences of HTTP requests with dependencies, retries, and response validation.

- [Crabflow 🦀](#crabflow-)
  - [Features ✨](#features-)
  - [Installation 📥](#installation-)
    - [Homebrew (MacOS/Linux (x86\_64) - x86\_64/arm64)](#homebrew-macoslinux-x86_64---x86_64arm64)
    - [Podman (Linux x86\_64)](#podman-linux-x86_64)
    - [Download and Run Binary](#download-and-run-binary)
    - [Build and Run Binary](#build-and-run-binary)
  - [Usage 🚀](#usage-)
  - [Workflow Configuration ⚙️](#workflow-configuration-️)
    - [Task Configuration](#task-configuration)
    - [Response Expectations](#response-expectations)
    - [Environment Variables](#environment-variables)
  - [License](#license)

## Features ✨

- 📝 Define workflows in YAML format
- 🌐 Execute HTTP requests with various methods (GET, POST, etc.)
- 📦 Support for different body types (JSON, form-urlencoded, raw, multipart)
- ✅ Response validation using status codes, JSON paths, and raw text matching
- 🔧 Environment variable resolution
- 🔄 Response registration and reference between tasks
- 🔁 Automatic retries with configurable delay
- 🔐 Basic authentication support
- 🎯 Custom headers support

## Installation 📥

### Homebrew (MacOS/Linux (x86_64) - x86_64/arm64)

```bash
brew install stenstromen/tap/crabflow
```

### Podman (Linux x86_64)

```bash
podman run --rm 
-e PASSWD=123 \
-e X_API_KEY=123 \
-e FOO=bar \
-v /path/to/workflow.yaml:/data/workflow.yaml:ro \
ghcr.io/stenstromen/crabflow:latest /crabflow /data/workflow.yaml
```

### Download and Run Binary

- For **MacOS** and **Linux**: Checkout and download the latest binary from [Releases page](https://github.com/Stenstromen/crabflow/releases/latest/)
- For **Windows**: Build the binary yourself.

### Build and Run Binary

```bash
cargo build --release
./target/release/crabflow
```

## Usage 🚀

Create a workflow file (e.g., `workflow.yaml`) and run it:

```bash
crabflow workflow.yaml
```

If no workflow file is specified, Crabflow will look for `workflow.yaml` in the current directory.

## Workflow Configuration ⚙️

Here's an example workflow configuration:

```yaml
name: example-pipeline
tasks:
  - name: fetch-users
    type: http
    method: GET
    url: http://api.example.com/users
    retries: 2
    expect:
      type: JsonPath
      path: data.users
      value: "[]"

  - name: create-user
    type: http
    method: POST
    url: http://api.example.com/users
    depends_on: [fetch-users]
    body_type: json
    body:
      name: "John Doe"
      email: "john@example.com"
    expect:
      - type: Status
        code: 201
      - type: JsonPath
        path: "id"
        value: "123"
    register: new_user

  - name: verify-user
    type: http
    method: GET
    url: http://api.example.com/users/{{new_user.json.id}}
    depends_on: [create-user]
    expect:
      - type: Status
        code: 200
      - type: JsonPath
        path: "name"
        value: "John Doe"

  - name: display-user
    type: display
    variables:
      - new_user
```

### Task Configuration

Each task in the workflow can have the following properties:

- `name`: Unique identifier for the task
- `type`: Type of task (currently only "http" is supported)
- `method`: HTTP method (GET, POST, PUT, DELETE, etc.)
- `url`: Target URL
- `headers`: Custom HTTP headers
- `body`: Request body (optional)
- `body_type`: Type of body (json, form-urlencoded, raw, form-multipart)
- `depends_on`: List of task names that must complete before this task
- `retries`: Number of retry attempts (default: 1)
- `retry_delay`: Delay between retries in seconds (default: 5)
- `expect`: List of expectations for the response
- `register`: Name to register the response for reference in other tasks
- `auth`: Basic authentication credentials

### Response Expectations

You can validate responses using:

- `Status`: Expected HTTP status code
- `JsonPath`: Expected value at a JSON path
- `Raw`: Expected text in the response

### Environment Variables

Use environment variables in your workflow:

```yaml
url: http://api.example.com/{{env.API_VERSION}}/users
headers:
  Authorization: "Bearer {{env.API_KEY}}"
```

## License

MIT License
