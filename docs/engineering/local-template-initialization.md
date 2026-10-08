# Local template initialization

Creating a ForkLaunch scaffold writes public framework templates on the user's computer. It does not read an account or create, change, or deploy a platform application. For example, creating a booking app's empty project folder should work before logging in to the deployment platform.

`init application` previously called `get_token()` and discarded its result. That unused credential-presence check prevented the desktop's network-isolated local scaffolder from running, while `init service` and `init worker` already worked without it. Local application initialization now follows the same rule. Authentication on account, release, deployment, and other protected API operations is unchanged.

Run `cli/tests/init_application_offline.sh` against a built CLI. The strongest acceptance run uses a clean container with `--network none`, a private empty HOME, no mounted credentials, and a read-only source/toolchain. The test initializes an application and service and verifies no login file was created. It skips the optional formatter because formatting and functional checks run separately in the desktop's restricted build worker.
