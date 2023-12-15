# Nomad-vmonitor
Monitors your Nomad-Cluster for new Versions of Software that is deployed

## Metrics Endpoint
Listens on `0.0.0.0:3000` and provides a `/metrics` endpoint to query prometheus metrics

## Environment Variables
* `NOMAD_ADDR`: The Nomad Server Address (defaults to localhost)
* `NOMAD_PORT`: The Nomad Server Port (defaults to 4646

## How it works
It periodically loads all the current Jobs registered in Nomad and then goes through them
based on their Task Drivers. The given Information is then compared to newly queried information
to determine if the currently configured version is the newest or not.
For example docker containers will be compared against the tags found on their registry.
