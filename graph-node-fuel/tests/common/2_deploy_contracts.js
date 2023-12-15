const Contract = artifacts.require('./Contract.sol')

module.exports = async function(deployer) {
  await deployer.deploy(Contract)
}
