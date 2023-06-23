# HA Setup

This example shows a HA setup.  
You can use it to test and see how `redhac` behaves when you control how / when nodes join or leave the cluster, and so
on.

The code will start only a single instance and you need to actually start it 3 times. If you put 3 terminals side by
side and then start the instances, you can see nicely, how each of them behaves in which situation.

To start each of the 3 nodes, execute:

`cargo run -- http://127.0.0.1:7071`

`cargo run -- http://127.0.0.1:7072`

`cargo run -- http://127.0.0.1:7073`

You can then observe the behaviorduring startup, the leader election, what happens when a nodes dies, and so on.  
After sleeping for 60 seconds, the host then will perform a graceful shutdown.
