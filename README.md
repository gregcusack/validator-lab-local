# Deploy Validator Clusters for Testing

## Disclaimer:
This library is a work in progress. It will be built over a series of PRs. Plan and progress for PRs is can be found [here](https://github.com/gregcusack/monogon-pr-plan/blob/main/README.md)

## How to run

### Setup
From your local build host, login to Docker for pushing/pulling repos. we assume auth for registryies are already setup.
```
docker login
```

```
kubectl create ns <namespace>
```

### Run
#### Build Agave from local agave repo
```
cargo run --bin cluster --
    -n <namespace>
    --deploy-method local
    --local-path <path-to-local-agave-monorepo>
    --do-build
```

#### Build specific Agave release
```
cargo run --bin cluster --
    -n <namespace>
    --deploy-method tar
    --release-channel <agave-version: e.g. v1.17.28> # note: MUST include the "v" 
```