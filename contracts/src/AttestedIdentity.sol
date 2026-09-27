// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import {IIdentityVerifier} from "./LifeRegistry.sol";

/// @title AttestedIdentity
/// @notice The research preview's identity adapter. An allow-listed attester (the operator, after a
/// document check by video call) signs an EIP-712 statement: member `memberId` is alive and holds
/// the identity of cohort `key` (birth year, sex, country), and authorises `action` (plain
/// identification, or a recovery to a new payout address and passkey). The statement is dated, lasts
/// at most 30 days, is bound to one registry on one chain, and the registry accepts each one once.
/// It is a trusted signer, and that is disclosed. Trust-minimised adapters (ZKPassport, signed
/// national-ID QR) implement the same interface.
contract AttestedIdentity is IIdentityVerifier, EIP712 {
    bytes32 public constant IDENTITY_TYPEHASH =
        keccak256("Identity(uint256 memberId,uint256 key,address registry,bytes32 action,uint64 issuedAt,uint64 expiry)");

    address public immutable attester;
    address public immutable registry;

    constructor(address attester_, address registry_) EIP712("Tonti Identity", "2") {
        attester = attester_;
        registry = registry_;
    }

    /// @param proof abi.encode(uint64 issuedAt, uint64 expiry, bytes signature)
    /// @return issuedAt when the statement was signed, or 0 if it is invalid
    function verify(uint256 memberId, uint256 key, bytes32 action, bytes calldata proof) external view returns (uint64) {
        (uint64 issuedAt, uint64 expiry, bytes memory signature) = abi.decode(proof, (uint64, uint64, bytes));
        if (issuedAt == 0 || issuedAt > block.timestamp || block.timestamp > expiry || expiry > uint256(issuedAt) + 30 days) {
            return 0;
        }
        bytes32 digest = _hashTypedDataV4(keccak256(abi.encode(IDENTITY_TYPEHASH, memberId, key, registry, action, issuedAt, expiry)));
        (address signer, ECDSA.RecoverError err,) = ECDSA.tryRecover(digest, signature);
        return err == ECDSA.RecoverError.NoError && signer == attester ? issuedAt : 0;
    }
}
