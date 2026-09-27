// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {P256} from "@openzeppelin/contracts/utils/cryptography/P256.sol";
import {Base64} from "@openzeppelin/contracts/utils/Base64.sol";
import {IERC20Min} from "./interfaces/External.sol";

/// @notice An identity adapter (a signed attestation, ZKPassport, a signed national-ID QR, ...),
/// allow-listed by governance. It proves the person behind `memberId` is alive and holds the
/// identity the pool priced: `key` = (isoNumeric·2 + sex)·10000 + birthYear. Binding the key is
/// what stops anyone claiming to be older than they are, the classic tontine fraud. `action` binds
/// what the statement authorises: `IDENTIFY` (zero) for a plain proof, or the hash of a recovery
/// (a new payout address and passkey). Returns when the statement was issued, or 0 if it is invalid.
interface IIdentityVerifier {
    function verify(uint256 memberId, uint256 key, bytes32 action, bytes calldata proof) external view returns (uint64 issuedAt);
}

/// @title Tonti LifeRegistry
/// @notice Proof of life for every annuitant: the pool must never pay the dead (docs/protocol.md §6).
///
///   ACTIVE ─(90d)→ DUE ─(+30d grace)→ LAPSED ─(24 months)→ PRESUMED_DECEASED
///   any ─(bonded death report)→ DEATH_REPORTED ─(120d unanswered)→ DECEASED
///   DECEASED or PRESUMED_DECEASED ─(identity proof within 5 years)→ alive again, repaid by the pool
///
/// Check-ins are passkey (WebAuthn, P-256 through the RIP-7212 precompile) signatures over a
/// per-member nonce; 2-of-3 guardian attestations also count as proof of life. Strong proofs come
/// only from allow-listed identity adapters, which bind the member's cohort (birth year, sex,
/// country). Income and exits need all three: ACTIVE or DUE, an identity once verified, and a
/// strong proof within `strongPeriod`.
///
/// The registry also keeps where each member's money goes (payout address and beneficiary) and
/// their guardians. The payout address can change all three; a member who lost their phone or
/// wallet recovers with an identity statement bound to the new payout address and passkey.
contract LifeRegistry {
    enum Status {
        None,
        Active,
        Due,
        Lapsed,
        DeathReported,
        Deceased,
        PresumedDeceased
    }

    struct Life {
        uint256 key; // the cohort the pool priced: (isoNumeric·2 + sex)·10000 + birthYear
        bool identified; // an identity adapter has confirmed the key
        bytes32 qx;
        bytes32 qy;
        address payout; // where income, exits and a rejected reporter's bond go
        address beneficiary; // who inherits the bequest share and the estate
        uint64 lastProof;
        uint64 lastStrong; // when the latest accepted identity statement was issued
        uint64 dateOfDeath;
        uint32 nonce;
        bool deceased;
        bool presumed;
        address[3] guardians;
    }

    struct Report {
        address reporter;
        uint64 filedAt;
        uint64 dateOfDeath;
        bytes32 evidence;
        uint256 bond;
    }

    /// A final report's bond, held for a year in case the member was alive after all.
    struct HeldBond {
        address reporter;
        uint64 releasableAt;
        uint256 amount;
    }

    /// @dev A WebAuthn assertion. `challengeIndex` and `typeIndex` locate the fields in clientDataJSON.
    struct WebAuthnAuth {
        bytes authenticatorData;
        string clientDataJSON;
        uint256 challengeIndex;
        uint256 typeIndex;
        bytes32 r;
        bytes32 s;
    }

    /// What a plain identity statement authorises: identification and proof of life, nothing else.
    bytes32 public constant IDENTIFY = bytes32(0);
    /// A newly allowed identity adapter can be used only after this delay (removal is immediate), so
    /// nobody can quietly add an adapter that identifies anyone.
    uint64 public constant VERIFIER_DELAY = 30 days;
    /// A death, reported or presumed, can be undone by an identity proof within this window.
    uint64 public constant REVIVAL_WINDOW = 5 * 365 days;
    /// A final report's bond is held this long; a revival in that time pays it to the member.
    uint64 public constant BOND_HOLD = 365 days;

    IERC20Min public immutable usdg;
    address public pool;
    address public owner;

    uint64 public checkInPeriod = 90 days;
    uint64 public grace = 30 days;
    uint64 public presumption = 730 days;
    /// Long enough that a member who checks in on schedule answers any report without knowing of it.
    uint64 public challengeWindow = 120 days;
    uint64 public strongPeriod = 400 days;
    uint256 public reportBond = 10e6; // 10 USDG

    mapping(uint256 => Life) internal lives;
    mapping(uint256 => Report) public reports;
    mapping(uint256 => HeldBond) public heldBonds;
    /// When each identity adapter becomes usable (0: not allowed).
    mapping(address => uint64) public verifierActiveAt;
    /// Who reported a death that became final (zero for presumed deaths).
    mapping(uint256 => address) public reporterOf;
    /// When a death became final (reported or presumed); revivals are counted from here.
    mapping(uint256 => uint64) public deceasedAt;
    /// When a member whose death was final proved who they are; the pool then repays them.
    mapping(uint256 => uint64) public revivedAt;
    /// memberId => guardian => timestamp of their latest attestation
    mapping(uint256 => mapping(address => uint64)) public attestedAt;
    /// The member's own latest proof of life: their passkey or an identity statement. Guardians
    /// keep a member from lapsing, but only this answers a death report.
    mapping(uint256 => uint64) public lastOwnProof;
    /// A requested recovery: the new payout address and passkey, and when it can take effect.
    struct Recovery {
        address payout;
        bytes32 qx;
        bytes32 qy;
        uint64 readyAt;
        address verifier;
        uint64 issuedAt;
    }
    mapping(uint256 => Recovery) public pendingRecovery;
    /// The issue date of the latest recovery statement used, cancelled or not. Recovery statements
    /// are tracked apart from `lastStrong`: a request someone else cancels must not count as the
    /// member's identity proof (it would reopen income to whoever holds the account).
    mapping(uint256 => uint64) public lastRecoveryIssued;

    error Unauthorized();
    error UnknownMember();
    error BadSignature();
    error NotAllowed();
    error ReportPending();

    event Enrolled(uint256 indexed memberId, address indexed payout, uint256 key);
    event PayoutChanged(uint256 indexed memberId, address indexed payout);
    event BeneficiarySet(uint256 indexed memberId, address indexed beneficiary);
    event GuardianSet(uint256 indexed memberId, address indexed guardian, uint8 slot);
    event PasskeyChanged(uint256 indexed memberId);
    event Recovered(uint256 indexed memberId, address verifier);
    event RecoveryRequested(uint256 indexed memberId, address indexed payout, uint64 readyAt, address verifier);
    event RecoveryCancelled(uint256 indexed memberId);
    event CheckedIn(uint256 indexed memberId, uint32 nonce);
    event StrongProof(uint256 indexed memberId, address verifier, uint64 issuedAt);
    event Identified(uint256 indexed memberId, address verifier);
    event GuardiansAttested(uint256 indexed memberId);
    event Attested(uint256 indexed memberId, address indexed guardian);
    event DeathReported(uint256 indexed memberId, address indexed reporter, uint64 dateOfDeath, bytes32 evidence);
    event ReportRejected(uint256 indexed memberId, address indexed reporter);
    event Deceased(uint256 indexed memberId, uint64 dateOfDeath, bool presumed);
    event Revived(uint256 indexed memberId, address verifier);
    event BondReleased(uint256 indexed memberId, address indexed to, uint256 amount);
    event VerifierSet(address indexed verifier, bool allowed, uint64 activeAt);

    modifier onlyPool() {
        if (msg.sender != pool) revert Unauthorized();
        _;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    constructor(address usdg_) {
        usdg = IERC20Min(usdg_);
        owner = msg.sender;
    }

    // ------------------------------------------------------------------ governance (bounded)

    function setPool(address pool_) external onlyOwner {
        require(pool == address(0), "pool set");
        pool = pool_;
    }

    function transferOwnership(address to) external onlyOwner {
        owner = to;
    }

    /// @notice Allows or removes an identity adapter. Removal is immediate. A new adapter becomes
    /// usable only `VERIFIER_DELAY` after it is allowed, except during set-up, before the pool is
    /// connected, when nobody is enrolled yet.
    function setVerifier(address verifier, bool allowed) external onlyOwner {
        uint64 at = !allowed ? 0 : pool == address(0) ? uint64(block.timestamp) : uint64(block.timestamp) + VERIFIER_DELAY;
        verifierActiveAt[verifier] = at;
        emit VerifierSet(verifier, allowed, at);
    }

    function strongVerifiers(address verifier) public view returns (bool) {
        uint64 at = verifierActiveAt[verifier];
        return at != 0 && block.timestamp >= at;
    }

    /// @notice How often an identity-bound strong proof is needed for income. Bounded: 180–730 days.
    function setStrongPeriod(uint64 period) external onlyOwner {
        require(period >= 180 days && period <= 730 days, "strong");
        strongPeriod = period;
    }

    /// @notice The challenge window can't be shorter than the check-in period plus grace, so a
    /// member who checks in on schedule always answers a report in time.
    function setPeriods(uint64 checkIn_, uint64 grace_, uint64 presumption_, uint64 window_) external onlyOwner {
        require(checkIn_ >= 30 days && checkIn_ <= 180 days, "checkIn");
        require(grace_ >= 7 days && grace_ <= 90 days, "grace");
        require(presumption_ >= 365 days && presumption_ <= 1825 days, "presumption");
        require(window_ >= checkIn_ + grace_ && window_ <= 365 days, "window");
        (checkInPeriod, grace, presumption, challengeWindow) = (checkIn_, grace_, presumption_, window_);
    }

    // ------------------------------------------------------------------ enrollment and accounts

    /// @notice Called by the pool when a member joins, with the cohort `key` it priced them in. The
    /// member is alive (someone just joined for them) but not yet identified: no income or exit
    /// until an identity adapter confirms the key.
    function enroll(
        uint256 memberId,
        uint256 key,
        bytes32 qx,
        bytes32 qy,
        address payout,
        address beneficiary,
        address[3] calldata guardians
    ) external onlyPool {
        Life storage l = lives[memberId];
        require(l.lastProof == 0, "enrolled");
        require(P256.isValidPublicKey(qx, qy), "passkey");
        require(payout != address(0) && beneficiary != address(0), "address");
        (l.key, l.qx, l.qy, l.payout, l.beneficiary) = (key, qx, qy, payout, beneficiary);
        l.lastProof = uint64(block.timestamp);
        lastOwnProof[memberId] = uint64(block.timestamp);
        emit Enrolled(memberId, payout, key);
        emit BeneficiarySet(memberId, beneficiary);
        _setGuardians(l, memberId, guardians);
    }

    /// @notice The payout address moves the account: a new payout address...
    function setPayout(uint256 memberId, address payout) external {
        Life storage l = _owned(memberId);
        require(payout != address(0), "address");
        l.payout = payout;
        emit PayoutChanged(memberId, payout);
    }

    /// @notice ... a new beneficiary ...
    function setBeneficiary(uint256 memberId, address beneficiary) external {
        Life storage l = _owned(memberId);
        require(beneficiary != address(0), "address");
        l.beneficiary = beneficiary;
        emit BeneficiarySet(memberId, beneficiary);
    }

    /// @notice ... or new guardians.
    function setGuardians(uint256 memberId, address[3] calldata guardians) external {
        _setGuardians(_owned(memberId), memberId, guardians);
    }

    /// @notice A member who lost their phone or wallet: an identity statement bound to the new payout
    /// address and passkey (the operator's video check, or a trust-minimised adapter) asks to replace
    /// both. It takes effect after the challenge window (`finishRecovery`, anyone may call), the
    /// same time a member has to answer a death report and at least a check-in period plus grace,
    /// unless the member checks in with their current passkey first. A member on her normal
    /// schedule therefore always checks in, and cancels it, before a misused statement (a
    /// compromised attester, say) can take effect. Income waits while a recovery is pending.
    function recover(uint256 memberId, address payout, bytes32 qx, bytes32 qy, address verifier, bytes calldata proof) external {
        Life storage l = _live(memberId);
        require(payout != address(0), "address");
        require(P256.isValidPublicKey(qx, qy), "passkey");
        _request(memberId, Recovery(payout, qx, qy, 0, verifier, _strong(l, verifier, memberId, recoveryAction(payout, qx, qy), proof)));
    }

    function _request(uint256 memberId, Recovery memory r) internal {
        if (r.issuedAt <= lastRecoveryIssued[memberId]) revert BadSignature(); // each statement counts once
        lastRecoveryIssued[memberId] = r.issuedAt;
        r.readyAt = uint64(block.timestamp) + challengeWindow;
        pendingRecovery[memberId] = r;
        emit RecoveryRequested(memberId, r.payout, r.readyAt, r.verifier);
    }

    /// @notice Applies a recovery whose delay has passed and that the member didn't cancel.
    function finishRecovery(uint256 memberId) external {
        Life storage l = _live(memberId);
        Recovery memory r = pendingRecovery[memberId];
        require(r.readyAt != 0, "none");
        require(block.timestamp >= r.readyAt, "waiting");
        delete pendingRecovery[memberId];
        (l.payout, l.qx, l.qy) = (r.payout, r.qx, r.qy);
        l.nonce++; // any check-in signed by the old passkey is void
        l.lastProof = uint64(block.timestamp);
        lastOwnProof[memberId] = uint64(block.timestamp);
        if (r.issuedAt > l.lastStrong) l.lastStrong = r.issuedAt; // now it was the member's proof
        emit Recovered(memberId, r.verifier);
        emit PayoutChanged(memberId, r.payout);
        emit PasskeyChanged(memberId);
    }

    /// @notice What a recovery statement must be bound to.
    function recoveryAction(address payout, bytes32 qx, bytes32 qy) public pure returns (bytes32) {
        return keccak256(abi.encode("tonti.recover", payout, qx, qy));
    }

    function _setGuardians(Life storage l, uint256 memberId, address[3] calldata guardians) internal {
        // Three different people (or empty slots): one address listed twice isn't 2 of 3.
        for (uint256 i; i < 3; i++) {
            for (uint256 j = i + 1; j < 3; j++) {
                require(guardians[i] == address(0) || guardians[i] != guardians[j], "guardians");
            }
        }
        l.guardians = guardians;
        for (uint8 i; i < 3; i++) {
            emit GuardianSet(memberId, guardians[i], i);
        }
    }

    // ------------------------------------------------------------------ proofs of life

    /// @notice The challenge a member's passkey must sign for the next check-in.
    function challenge(uint256 memberId) public view returns (bytes32) {
        return keccak256(abi.encode("tonti.checkin", block.chainid, address(this), memberId, lives[memberId].nonce));
    }

    /// @notice Cheap proof of life: a WebAuthn assertion from the member's enrolled passkey.
    function checkIn(uint256 memberId, WebAuthnAuth calldata auth) external {
        Life storage l = _live(memberId);
        if (!verifyWebAuthn(challenge(memberId), auth, l.qx, l.qy)) revert BadSignature();
        l.nonce++;
        l.lastProof = uint64(block.timestamp);
        lastOwnProof[memberId] = uint64(block.timestamp);
        emit CheckedIn(memberId, l.nonce);
        // The member still holds their passkey: a pending recovery wasn't them.
        if (pendingRecovery[memberId].readyAt != 0) {
            delete pendingRecovery[memberId];
            emit RecoveryCancelled(memberId);
        }
    }

    /// @notice Identity-bound proof through an allow-listed adapter: it confirms the member holds the
    /// identity of their cohort key and is alive. The first one identifies the member. Each
    /// statement counts once: it must be newer than the last one accepted.
    function strongProof(uint256 memberId, address verifier, bytes calldata proof) external {
        Life storage l = _live(memberId);
        uint64 issued = _strong(l, verifier, memberId, IDENTIFY, proof);
        if (!l.identified) {
            l.identified = true;
            emit Identified(memberId, verifier);
        }
        l.lastProof = uint64(block.timestamp);
        lastOwnProof[memberId] = uint64(block.timestamp);
        l.lastStrong = issued;
        emit StrongProof(memberId, verifier, issued);
    }

    /// @notice A guardian attests the member is alive. Two distinct guardians within 30 days count as a
    /// proof of life. Guardians are the member's own choice, so they can't vouch for identity or age.
    function attest(uint256 memberId) external {
        Life storage l = _live(memberId);
        uint256 idx = 3;
        for (uint256 i; i < 3; i++) {
            if (l.guardians[i] == msg.sender && msg.sender != address(0)) idx = i;
        }
        if (idx == 3) revert Unauthorized();
        attestedAt[memberId][msg.sender] = uint64(block.timestamp);
        emit Attested(memberId, msg.sender);
        uint256 recent;
        for (uint256 i; i < 3; i++) {
            address g = l.guardians[i];
            if (g != address(0) && block.timestamp - attestedAt[memberId][g] <= 30 days && attestedAt[memberId][g] != 0) {
                recent++;
            }
        }
        if (recent >= 2) {
            l.lastProof = uint64(block.timestamp);
            emit GuardiansAttested(memberId);
        }
    }

    // ------------------------------------------------------------------ deaths

    /// @notice Anyone may report a death with a USDG bond. The member defeats it with their own proof of
    /// life (their passkey or an identity statement; guardians can't answer for them)
    /// made after the report was filed; otherwise it becomes final after the challenge window.
    function reportDeath(uint256 memberId, uint64 dateOfDeath, bytes32 evidence) external {
        _live(memberId);
        if (reports[memberId].reporter != address(0)) revert ReportPending();
        require(dateOfDeath <= block.timestamp, "future");
        // Nobody died before their last proof of life (their own or their guardians'): an earlier
        // date would cut the estate's income, so it is moved up to it.
        if (dateOfDeath < lives[memberId].lastProof) dateOfDeath = lives[memberId].lastProof;
        require(usdg.transferFrom(msg.sender, address(this), reportBond), "bond");
        reports[memberId] = Report(msg.sender, uint64(block.timestamp), dateOfDeath, evidence, reportBond);
        emit DeathReported(memberId, msg.sender, dateOfDeath, evidence);
    }

    /// @notice Settle a report: rejected if the member proved life after it was filed (the bond goes to
    /// the member), final if the window passed unanswered. A final report's bond is held for a year:
    /// if the member proves they're alive in that time, it goes to them.
    function resolveReport(uint256 memberId) external {
        Report memory r = reports[memberId];
        require(r.reporter != address(0), "no report");
        Life storage l = lives[memberId];
        require(!l.deceased, "dead"); // a presumption closed it
        if (lastOwnProof[memberId] > r.filedAt) {
            delete reports[memberId];
            require(usdg.transfer(l.payout, r.bond), "bond");
            emit ReportRejected(memberId, r.reporter);
            return;
        }
        require(block.timestamp >= r.filedAt + challengeWindow, "window open");
        delete reports[memberId];
        l.deceased = true;
        l.dateOfDeath = r.dateOfDeath;
        reporterOf[memberId] = r.reporter;
        deceasedAt[memberId] = uint64(block.timestamp);
        revivedAt[memberId] = 0; // a revival from an earlier death can't repay this one
        delete pendingRecovery[memberId];
        heldBonds[memberId] = HeldBond(r.reporter, uint64(block.timestamp) + BOND_HOLD, r.bond);
        emit Deceased(memberId, r.dateOfDeath, false);
    }

    /// @notice Returns a final report's bond to its reporter once the hold has passed.
    function releaseBond(uint256 memberId) external {
        HeldBond memory b = heldBonds[memberId];
        require(b.amount != 0, "no bond");
        require(block.timestamp >= b.releasableAt, "held");
        delete heldBonds[memberId];
        require(usdg.transfer(b.reporter, b.amount), "bond");
        emit BondReleased(memberId, b.reporter, b.amount);
    }

    /// @notice Anyone may mark a member presumed dead after `presumption` without any proof of life,
    /// or, for an identified member, after two strong periods without an identity-bound proof.
    /// The second path matters: heirs holding a dead member's passkey can keep checking in, but they
    /// can't prove the dead member's identity. It is dated at the last identity proof.
    function presumeDeceased(uint256 memberId) external {
        Life storage l = _live(memberId);
        bool silent = block.timestamp > l.lastProof + presumption;
        bool unproven = l.identified && block.timestamp > uint256(l.lastStrong) + 2 * uint256(strongPeriod);
        require(silent || unproven, "not lapsed long enough");
        uint64 date = silent ? l.lastProof : l.lastStrong;
        l.deceased = true;
        l.presumed = true;
        l.dateOfDeath = date;
        deceasedAt[memberId] = uint64(block.timestamp);
        revivedAt[memberId] = 0; // a revival from an earlier death can't repay this one
        delete pendingRecovery[memberId];
        // A report still open is closed, its bond returned: the presumption stands for the death.
        Report memory r = reports[memberId];
        if (r.reporter != address(0)) {
            delete reports[memberId];
            require(usdg.transfer(r.reporter, r.bond), "bond");
        }
        emit Deceased(memberId, date, true);
    }

    /// @notice A member whose death became final, reported or presumed, proves who they are through an
    /// allow-listed identity adapter bound to their cohort key, within `REVIVAL_WINDOW`. They are
    /// alive again, the pool repays their released at-risk shares from its revival reserve, and a
    /// report's bond still held goes to them.
    function revive(uint256 memberId, address verifier, bytes calldata proof) external {
        Life storage l = lives[memberId];
        if (!l.deceased) revert NotAllowed();
        if (block.timestamp > uint256(deceasedAt[memberId]) + REVIVAL_WINDOW) revert NotAllowed();
        uint64 issued = _strong(l, verifier, memberId, IDENTIFY, proof);
        require(issued > deceasedAt[memberId], "stale");
        l.deceased = false;
        l.presumed = false;
        l.dateOfDeath = 0;
        l.identified = true;
        l.lastProof = uint64(block.timestamp);
        lastOwnProof[memberId] = uint64(block.timestamp);
        l.lastStrong = issued;
        revivedAt[memberId] = uint64(block.timestamp);
        emit Revived(memberId, verifier);
        HeldBond memory b = heldBonds[memberId];
        if (b.amount != 0) {
            delete heldBonds[memberId];
            require(usdg.transfer(l.payout, b.amount), "bond");
            emit BondReleased(memberId, l.payout, b.amount);
        }
    }

    // ------------------------------------------------------------------ views

    function status(uint256 memberId) public view returns (Status) {
        Life storage l = lives[memberId];
        if (l.lastProof == 0) return Status.None;
        if (l.deceased) return l.presumed ? Status.PresumedDeceased : Status.Deceased;
        if (reports[memberId].reporter != address(0)) return Status.DeathReported;
        uint256 since = block.timestamp - l.lastProof;
        if (since <= checkInPeriod) return Status.Active;
        if (since <= checkInPeriod + grace) return Status.Due;
        return Status.Lapsed;
    }

    /// @notice Income and exits need a live member (ACTIVE or DUE), whose identity an adapter has
    /// confirmed, with a strong proof within `strongPeriod`.
    function canReceiveIncome(uint256 memberId) external view returns (bool) {
        Status s = status(memberId);
        Life storage l = lives[memberId];
        // Income waits while a recovery is pending, so it goes to whoever the account turns out to be.
        return (s == Status.Active || s == Status.Due) && l.identified && block.timestamp <= uint256(l.lastStrong) + strongPeriod
            && pendingRecovery[memberId].readyAt == 0;
    }

    /// @notice Whether an identity adapter has confirmed the member's cohort. The pool takes no money
    /// before this, so nobody's savings can be stranded unverified.
    function identified(uint256 memberId) external view returns (bool) {
        return lives[memberId].identified;
    }

    /// @notice When the latest accepted identity statement was issued (0 if never). The pool uses it
    /// to release a group flagged by the ghost-member detector, member by member.
    function lastStrong(uint256 memberId) external view returns (uint64) {
        return lives[memberId].lastStrong;
    }

    function payoutOf(uint256 memberId) external view returns (address) {
        return lives[memberId].payout;
    }

    function beneficiaryOf(uint256 memberId) external view returns (address) {
        return lives[memberId].beneficiary;
    }

    function life(uint256 memberId) external view returns (Life memory) {
        return lives[memberId];
    }

    /// @notice Final date of death (0 while alive). The pool pays the estate only income for
    /// months up to this date; anything later returns to the pool.
    function dateOfDeath(uint256 memberId) external view returns (uint64) {
        Life storage l = lives[memberId];
        return l.deceased ? l.dateOfDeath : 0;
    }

    function _live(uint256 memberId) internal view returns (Life storage l) {
        l = lives[memberId];
        if (l.lastProof == 0) revert UnknownMember();
        if (l.deceased) revert NotAllowed();
    }

    /// A live member whose payout address is the caller.
    function _owned(uint256 memberId) internal view returns (Life storage l) {
        l = _live(memberId);
        if (msg.sender != l.payout) revert Unauthorized();
    }

    /// Checks an identity statement for `action` through an allowed adapter; returns when it was
    /// issued. A statement older than the last accepted one is refused, so none can be replayed.
    function _strong(Life storage l, address verifier, uint256 memberId, bytes32 action, bytes calldata proof)
        internal
        view
        returns (uint64 issued)
    {
        if (!strongVerifiers(verifier)) revert NotAllowed();
        issued = IIdentityVerifier(verifier).verify(memberId, l.key, action, proof);
        if (issued == 0) revert BadSignature();
        if (issued <= l.lastStrong) revert BadSignature();
    }

    // ------------------------------------------------------------------ WebAuthn

    /// @notice Verifies a WebAuthn assertion over `challenge_`: the user-present flag, type
    /// "webauthn.get", the base64url challenge in clientDataJSON, and the P-256 signature over
    /// authenticatorData ‖ sha256(clientDataJSON). Low-s is enforced by P256.verify.
    function verifyWebAuthn(bytes32 challenge_, WebAuthnAuth calldata a, bytes32 qx, bytes32 qy)
        public
        view
        returns (bool)
    {
        if (a.authenticatorData.length < 37) return false;
        if (uint8(a.authenticatorData[32]) & 0x01 == 0) return false; // user present
        bytes memory json = bytes(a.clientDataJSON);
        if (!_contains(json, a.typeIndex, bytes('"type":"webauthn.get"'))) return false;
        bytes memory expected = abi.encodePacked('"challenge":"', Base64.encodeURL(abi.encodePacked(challenge_)), '"');
        if (!_contains(json, a.challengeIndex, expected)) return false;
        bytes32 h = sha256(abi.encodePacked(a.authenticatorData, sha256(json)));
        return P256.verify(h, a.r, a.s, qx, qy);
    }

    function _contains(bytes memory hay, uint256 at, bytes memory needle) internal pure returns (bool) {
        if (at + needle.length > hay.length) return false;
        for (uint256 i; i < needle.length; i++) {
            if (hay[at + i] != needle[i]) return false;
        }
        return true;
    }
}
