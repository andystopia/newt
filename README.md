# Newt

The point of this here is that we want to 
have a system where I can easily search, compose, and 
improve the UX of existing nix experiments.

Current struggle. 

Nix flakes are complicated: 
	- they are a collection of devshells, each devshell is a collection of packages that is named and possibly has a startup script.
	- they are a collection of build environments, each build environment is a collection of packages and input builds and possibly
	many build scripts.
	- We want to be able to "trim the fat", if need be, and have lighter devshells for some circumstances and heavier ones
	in others. 
	


Nix terminology is not obvious what it is, like devshell does not really sound like the description above
inherently, so I want to simplify it, but I don't want to dumb it down so much that it loses it's flexibility.


