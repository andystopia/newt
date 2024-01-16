{

	inputs = {
		### <nix inputs> ### 

		### </nix inputs> ###
	};

	outputs = {self, 
	### <nix inputs parameters> ###
	### </nix inputs parameters> ###
	} : let
		supportedSystems = [ "x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin" ];
		in 
		{ 
		
		}
	;
}
