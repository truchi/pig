# Pig

> 🦀 OpenAPI code generation 🐷

## Install

```sh
cargo install --git git@github.com:truchi/pig.git --locked
```

## Usage

 ```text
🦀 OpenAPI code generation 🐷

Usage: pig [OPTIONS] [CONFIG]

Arguments:
  [CONFIG]  Path of the `pig.yaml` file (leave empty to search upwards from the current directory)

Options:
  -w, --watch    Watch mode
  -h, --help     Print help
  -V, --version  Print version
 ```

## Config

`Pig` uses a `pig.yaml` configuration file:

```yaml
# This entry tells `pig` to load `./openapi.yaml`
# as the context for `./templates/**/*.jinja`
# and render that into the `./output` directory:
- api: "openapi.yaml"
  in: "templates"
  out: "output"

# You can have as many entries as you want:
- api: "openapi.yaml"
  in: "templates"
  out: "../other/output"
```

## OpenAPI

`Pig` supports `OpenAPI` `v3.0.x`.

References are resolved into the referenced object, adding:
- `$ref`: the `$ref` string (file path resolved), e.g. `/path/to/file.yaml#/path/to/Object`
- `$file`: the file path part of the `$ref`, e.g. `/path/to/file.yaml`
- `$keys`: the key part of the `$ref` as an array, e.g. `["path", "to", "Object"]`
- `$name`: the last key of the `$ref`, e.g. `Object`

`Pig` detects circular references.

## Templates

`Pig` uses `Tera` as its template engine.

Two extra files are written to the output directory: `.pig.context.json` and `.pig.context.yaml`. Those contain the context given to templates, i.e. the `OpenAPI` specification.

## Links
- [https://github.com/truchi/pig]()
- [https://www.openapis.org]()
- [https://learn.openapis.org]()
- [https://openapi-map.apihandyman.io]()
- [https://keats.github.io/tera]()
- [https://keats.github.io/tera/docs]()
