# Nomad-vmonitor
Monitors your Nomad-Cluster for new Versions of Software to deploy

## How it works
It periodically loads all the current Jobs registered in Nomad and then goes through them
based on their Task Drivers. The given Information is then compared to newly queried information
to determine if the currently configured version is the newest or not.
For example docker containers will be compared against the tags found on their registry.