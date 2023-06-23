# Conflict Resolution Tests

This "example" is used for testing the conflict resolution of `redhac`.  
Doing it with an example is way easier to debug than with a real test.

This code will spin up 3 HA cache nodes on the same host in the exact same moment to produce as much conflicts during
the startup and initial leader election as possible. The way it is done will produce a lot more conflicts than it
would ever happen in the real world, where the cache members run on different hosts or locations, have network latency
between them, and so on.

The code is just looping through a lot of different cache startups to make sure each of the conflicts will be resolved
eventually.
