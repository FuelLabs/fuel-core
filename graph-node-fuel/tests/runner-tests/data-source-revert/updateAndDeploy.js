// This takes a Qm.. hash as a CLI input, which is the graft base.

const fs = require('fs');
const { execSync } = require('child_process');

const graftBase = process.argv[2];

const yamlPath = './grafted.yaml';
let yamlContent = fs.readFileSync(yamlPath, 'utf-8');
yamlContent = yamlContent.replace(/base: .+/, `base: ${graftBase}`);
fs.writeFileSync(yamlPath, yamlContent);
console.log("bafuzz")

// Assuming you have your IPFS_URI and GRAPH_NODE_ADMIN_URI exported as environment variables.
execSync('graph deploy test/data-source-revert-grafted grafted.yaml --version-label v0.0.1 --ipfs $IPFS_URI --node $GRAPH_NODE_ADMIN_URI', {
    stdio: 'inherit'
});
