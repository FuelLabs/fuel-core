{
  description = ''
    A Nix flake for compiling and extracting the fuel contract abi.
  '';

  inputs = {
    open-zeppelin = {
      url = "github:OpenZeppelin/openzeppelin-contracts";
      flake = false;
    };
    fuel-v1-contracts = {
      url = "github:FuelLabs/fuel-merkle-sol";
      flake = false;
    };
    fuel-v2-contracts.url = "git+ssh://git@github.com/fuellabs/fuel-v2-contracts/";
    fuel-v2-contracts.flake = false;
    nixpkgs = {
      url = "github:NixOS/nixpkgs/nixos-unstable";
    };
    dapptools = {
      url = "github:dapphub/dapptools/master";
      flake = false;
    };
    npm.url = "github:/serokell/nix-npm-buildpackage";
  };

  outputs = inputs: let
    system = "x86_64-darwin";
    dapp = import inputs.dapptools {inherit system;};
    solc = dapp.runCommand "solc" {} "mkdir -p $out/bin; ln -s ${dapp.solc-static-versions.solc_0_8_9}/bin/solc-0.8.9 $out/bin/solc";
    overlays = [inputs.npm.overlays.default];
    pkgs = import inputs.nixpkgs {inherit overlays system;};
  in {
    packages."x86_64-darwin" = {
      add-folder = pkgs.linkFarm "contracts-with-layer" [
        {
          name = "merkle-sol";
          path = inputs.fuel-v1-contracts;
        }
      ];
      build-abi-json =
        pkgs.runCommand
        "build-abi-json"
        {}
        ''
          ${solc}/bin/solc --abi --pretty-json ${inputs.fuel-v2-contracts}/contracts/sidechain/FuelSidechain.sol @openzeppelin=${inputs.open-zeppelin} @fuel-contracts=${inputs.self.packages."${system}".add-folder} -o $out/build
        '';
      generate-abi-json = pkgs.writeShellApplication {
        name = "generate-abi-0json";
        runtimeInputs = [solc pkgs.jq inputs.self.packages."${system}".build-abi-json];
        text = ''
          if [[ -z $1 ]] 
          then 
            echo "Invalid input. Must provide output directoy" >&2; exit 1;
          fi
          declare -a arr=("FuelMessagePortal")
          for i in "''${arr[@]}"
          do  
            (cat ${inputs.self.packages."${system}".build-abi-json}/build/"$i".abi) | jq '.' > "$1"/"$i".json
          done
        '';
      };

      
    };
    formatter.x86_64-darwin = pkgs.alejandra;
  };
}
