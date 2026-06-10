
The purpose of the project is to build a dynamic dns (ddns) client like ddclient, but much less general purpose, and targeted specifically to the limited set of scenarios needed.

The client is a command-line rust program that fetches the ip address visible externally using an https endpoint that returns the ip address, and updates my dynamic dns provider (cloudflare) 

