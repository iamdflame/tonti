// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {Base64} from "@openzeppelin/contracts/utils/Base64.sol";
import {LifeRegistry} from "../src/LifeRegistry.sol";
import {AttestedIdentity} from "../src/AttestedIdentity.sol";

/// Minimal USDG stand-in for bonds in unit tests; fork tests use the real token.
contract TestUsdg {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 a) external {
        balanceOf[to] += a;
    }

    function approve(address s, uint256 a) external returns (bool) {
        allowance[msg.sender][s] = a;
        return true;
    }

    function transfer(address to, uint256 a) external returns (bool) {
        balanceOf[msg.sender] -= a;
        balanceOf[to] += a;
        return true;
    }

    function transferFrom(address f, address t, uint256 a) external returns (bool) {
        allowance[f][msg.sender] -= a;
        balanceOf[f] -= a;
        balanceOf[t] += a;
        return true;
    }

    function decimals() external pure returns (uint8) {
        return 6;
    }
}

contract LifeRegistryTest is Test {
    uint256 constant N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551;
    uint256 constant MEMBER = 1;
    uint256 constant PASSKEY = 0xA11CE5EC7E7;
    uint256 constant KEY = 12161966; // Philippines, female, born 1966
    uint256 constant ATTESTER = 0xA77E57;
    bytes32 constant IDENTIFY = bytes32(0);

    LifeRegistry reg;
    AttestedIdentity idv;
    TestUsdg usdg;
    address pool = makeAddr("pool");
    address payoutAddr = makeAddr("maria");
    address heir = makeAddr("heir");
    address g1 = makeAddr("sister");
    address g2 = makeAddr("pastor");
    address g3 = makeAddr("friend");
    address reporter = makeAddr("reporter");

    function setUp() public {
        vm.warp(1_790_460_000); // 2026-09-26
        usdg = new TestUsdg();
        reg = new LifeRegistry(address(usdg));
        idv = new AttestedIdentity(vm.addr(ATTESTER), address(reg));
        // During set-up, before the pool is connected, a verifier is usable at once.
        reg.setVerifier(address(idv), true);
        reg.setPool(pool);
        _enroll(MEMBER, PASSKEY);
        // The liveness tests below start from an identified member.
        reg.strongProof(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        usdg.mint(reporter, 100e6);
        vm.prank(reporter);
        usdg.approve(address(reg), type(uint256).max);
    }

    function _enroll(uint256 id, uint256 passkey) internal {
        (uint256 qx, uint256 qy) = vm.publicKeyP256(passkey);
        vm.prank(pool);
        reg.enroll(id, KEY, bytes32(qx), bytes32(qy), payoutAddr, heir, [g1, g2, g3]);
    }

    /// The attester's EIP-712 statement, issued now and valid for 7 days.
    function _attest(uint256 memberId, uint256 key, bytes32 action, uint256 signer) internal view returns (bytes memory) {
        return _attestAt(memberId, key, action, uint64(block.timestamp), uint64(block.timestamp + 7 days), signer);
    }

    function _attestAt(uint256 memberId, uint256 key, bytes32 action, uint64 issuedAt, uint64 expiry, uint256 signer)
        internal
        view
        returns (bytes memory)
    {
        return _attestVia(address(idv), memberId, key, action, issuedAt, expiry, signer);
    }

    /// The statement as signed for a given adapter contract (its EIP-712 domain).
    function _attestVia(address adapter, uint256 memberId, uint256 key, bytes32 action, uint64 issuedAt, uint64 expiry, uint256 signer)
        internal
        view
        returns (bytes memory)
    {
        bytes32 domain = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256("Tonti Identity"),
                keccak256("2"),
                block.chainid,
                adapter
            )
        );
        bytes32 digest = keccak256(
            abi.encodePacked(
                "\x19\x01",
                domain,
                keccak256(abi.encode(idv.IDENTITY_TYPEHASH(), memberId, key, address(reg), action, issuedAt, expiry))
            )
        );
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(signer, digest);
        return abi.encode(issuedAt, expiry, abi.encodePacked(r, s, v));
    }

    /// Builds a WebAuthn assertion the way a browser does, and signs it with the passkey.
    function _assertion(bytes32 challenge, uint256 key) internal view returns (LifeRegistry.WebAuthnAuth memory a) {
        a.authenticatorData = abi.encodePacked(sha256("tonti.app"), uint8(0x05), uint32(1)); // UP | UV
        string memory prefix = '{"type":"webauthn.get",';
        string memory json = string.concat(
            prefix, '"challenge":"', Base64.encodeURL(abi.encodePacked(challenge)), '","origin":"https://tonti.app"}'
        );
        a.clientDataJSON = json;
        a.typeIndex = 1;
        a.challengeIndex = bytes(prefix).length;
        bytes32 h = sha256(abi.encodePacked(a.authenticatorData, sha256(bytes(json))));
        (bytes32 r, bytes32 s) = vm.signP256(key, h);
        if (uint256(s) > N / 2) s = bytes32(N - uint256(s)); // low-s, as authenticators are normalized
        (a.r, a.s) = (r, s);
    }

    function test_checkInWithPasskeyKeepsMemberActive() public {
        vm.warp(block.timestamp + 80 days);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        vm.warp(block.timestamp + 80 days);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active));
        assertTrue(reg.canReceiveIncome(MEMBER));
    }

    function test_replayedOrForeignSignatureIsRejected() public {
        LifeRegistry.WebAuthnAuth memory a = _assertion(reg.challenge(MEMBER), PASSKEY);
        reg.checkIn(MEMBER, a);
        vm.expectRevert(LifeRegistry.BadSignature.selector); // nonce advanced: replay fails
        reg.checkIn(MEMBER, a);
        // Build first: expectRevert applies to the next external call, and challenge() is one.
        LifeRegistry.WebAuthnAuth memory foreign = _assertion(reg.challenge(MEMBER), 0xBAD);
        vm.expectRevert(LifeRegistry.BadSignature.selector); // someone else's passkey
        reg.checkIn(MEMBER, foreign);
    }

    function test_missedCheckInsHoldIncomeThenPresumeDeath() public {
        vm.warp(block.timestamp + 100 days);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Due));
        assertTrue(reg.canReceiveIncome(MEMBER));
        vm.warp(block.timestamp + 30 days);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Lapsed));
        assertFalse(reg.canReceiveIncome(MEMBER), "income held while lapsed");
        vm.expectRevert(bytes("not lapsed long enough"));
        reg.presumeDeceased(MEMBER);
        vm.warp(block.timestamp + 730 days);
        reg.presumeDeceased(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.PresumedDeceased));
    }

    function test_falseDeathReportIsDefeatedAndBondGoesToMember() public {
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("forged certificate"));
        assertFalse(reg.canReceiveIncome(MEMBER), "income held during a report");
        vm.warp(block.timestamp + 3 days);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        reg.resolveReport(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active));
        assertEq(usdg.balanceOf(payoutAddr), 10e6, "bond paid to the member");
    }

    /// The skeptic's attack: a report filed just after a check-in. A member who simply keeps her
    /// 90-day schedule, never knowing of the report, answers it: the window is 120 days.
    function test_aMemberOnScheduleAnswersAReportWithoutKnowingOfIt() public {
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        vm.warp(block.timestamp + 1 hours);
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("forged certificate"));
        vm.warp(block.timestamp + 90 days);
        vm.expectRevert(bytes("window open"));
        reg.resolveReport(MEMBER);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY)); // her usual check-in
        reg.resolveReport(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active));
        assertEq(usdg.balanceOf(payoutAddr), 10e6, "and the attacker's bond is hers");
    }

    function test_unansweredDeathReportBecomesFinalAndTheBondIsHeldForAYear() public {
        uint64 died = uint64(block.timestamp);
        vm.prank(reporter);
        reg.reportDeath(MEMBER, died, keccak256("certificate"));
        vm.warp(block.timestamp + 119 days);
        vm.expectRevert(bytes("window open"));
        reg.resolveReport(MEMBER);
        vm.warp(block.timestamp + 1 days);
        reg.resolveReport(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Deceased));
        assertEq(reg.life(MEMBER).dateOfDeath, died);
        assertEq(reg.reporterOf(MEMBER), reporter);
        assertEq(usdg.balanceOf(reporter), 90e6, "the bond is held");
        vm.expectRevert(bytes("held"));
        reg.releaseBond(MEMBER);
        vm.warp(block.timestamp + 365 days);
        reg.releaseBond(MEMBER);
        assertEq(usdg.balanceOf(reporter), 100e6, "an honest reporter gets it back after a year");
        LifeRegistry.WebAuthnAuth memory late = _assertion(reg.challenge(MEMBER), PASSKEY);
        vm.expectRevert(LifeRegistry.NotAllowed.selector); // the dead cannot check in
        reg.checkIn(MEMBER, late);
    }

    /// A false report that went unanswered (she was in hospital, say) is undone by an identity
    /// proof, and the reporter's held bond goes to her.
    function test_aFinalReportedDeathCanBeUndoneAndTheBondGoesToTheMember() public {
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("forged certificate"));
        vm.warp(block.timestamp + 120 days);
        reg.resolveReport(MEMBER);
        bytes memory before = _attest(MEMBER, KEY, IDENTIFY, ATTESTER);
        vm.expectRevert(bytes("stale")); // a statement from before the death became final won't do
        reg.revive(MEMBER, address(idv), before);
        vm.warp(block.timestamp + 40 days);
        reg.revive(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active));
        assertTrue(reg.canReceiveIncome(MEMBER));
        assertEq(reg.revivedAt(MEMBER), block.timestamp, "the pool repays her from its reserve");
        assertEq(usdg.balanceOf(payoutAddr), 10e6, "the reporter's bond is hers");
        vm.expectRevert(bytes("no bond"));
        reg.releaseBond(MEMBER);
    }

    function test_twoGuardiansProveLifeButNotIdentity() public {
        vm.warp(block.timestamp + 200 days);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Lapsed));
        vm.prank(g1);
        reg.attest(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Lapsed), "one guardian is not enough");
        vm.prank(g3);
        reg.attest(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active));
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        reg.attest(MEMBER); // a stranger

        // A new member whose guardians vouch for them is alive, but not identified: no income.
        _enroll(2, 0xB0B);
        vm.prank(g1);
        reg.attest(2);
        vm.prank(g2);
        reg.attest(2);
        assertEq(uint8(reg.status(2)), uint8(LifeRegistry.Status.Active));
        assertFalse(reg.canReceiveIncome(2), "guardians can't vouch for identity or age");
    }

    function test_incomeWaitsForAnIdentityBoundToTheCohortKey() public {
        _enroll(2, 0xB0B);
        assertEq(uint8(reg.status(2)), uint8(LifeRegistry.Status.Active));
        assertFalse(reg.canReceiveIncome(2), "joined, not yet identified");
        uint64 t = uint64(block.timestamp);
        bytes32 recovery = reg.recoveryAction(payoutAddr, bytes32(uint256(1)), bytes32(uint256(2)));
        // Each of these fails: claiming to be born in 1946 (older, so paid more), someone else's
        // statement, an expired one, one valid too long, one issued in the future, one signed by
        // someone else, and a recovery statement used as an identification.
        bytes[7] memory bad = [
            _attestAt(2, 12161946, IDENTIFY, t, t + 7 days, ATTESTER),
            _attestAt(MEMBER, KEY, IDENTIFY, t, t + 7 days, ATTESTER),
            _attestAt(2, KEY, IDENTIFY, t - 8 days, t - 1, ATTESTER),
            _attestAt(2, KEY, IDENTIFY, t, t + 60 days, ATTESTER),
            _attestAt(2, KEY, IDENTIFY, t + 1 hours, t + 7 days, ATTESTER),
            _attestAt(2, KEY, IDENTIFY, t, t + 7 days, 0xBAD),
            _attestAt(2, KEY, recovery, t, t + 7 days, ATTESTER)
        ];
        for (uint256 i; i < bad.length; i++) {
            vm.expectRevert(LifeRegistry.BadSignature.selector);
            reg.strongProof(2, address(idv), bad[i]);
        }
        AttestedIdentity rogue = new AttestedIdentity(vm.addr(0xBAD), address(reg));
        bytes memory rogueProof = _attest(2, KEY, IDENTIFY, 0xBAD);
        vm.expectRevert(LifeRegistry.NotAllowed.selector);
        reg.strongProof(2, address(rogue), rogueProof);
        assertFalse(reg.canReceiveIncome(2));
        bytes memory good = _attest(2, KEY, IDENTIFY, ATTESTER);
        reg.strongProof(2, address(idv), good);
        assertTrue(reg.life(2).identified);
        assertTrue(reg.canReceiveIncome(2));
        // The same statement can't be used twice (it would stretch the strong period).
        vm.warp(block.timestamp + 1 days);
        vm.expectRevert(LifeRegistry.BadSignature.selector);
        reg.strongProof(2, address(idv), good);
    }

    function test_passkeyCheckInsAloneLapseAfterTheStrongPeriod() public {
        for (uint256 i; i < 6; i++) {
            vm.warp(block.timestamp + 80 days);
            reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        }
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active), "checked in every 80 days");
        assertFalse(reg.canReceiveIncome(MEMBER), "480 days since the last identity-bound proof");
        reg.strongProof(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        assertTrue(reg.canReceiveIncome(MEMBER));
        vm.expectRevert(bytes("strong"));
        reg.setStrongPeriod(30 days);
        // Skeptic 4: shortening it would hold income of, and presume dead, members who renew
        // yearly. It can only be lengthened.
        vm.expectRevert(bytes("strong"));
        reg.setStrongPeriod(399 days);
        reg.setStrongPeriod(500 days);
    }

    function test_passkeyGhostsArePresumedWhenIdentityProofsLapseAndTheLivingCanRevive() public {
        // Heirs hold the dead member's passkey and keep checking in every 80 days...
        for (uint256 i; i < 11; i++) {
            vm.warp(block.timestamp + 80 days);
            reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        }
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active), "check-ins keep the account active");
        // ...but after two strong periods (800 days) with no identity proof, anyone may presume death.
        uint64 lastStrong = reg.lastStrong(MEMBER);
        reg.presumeDeceased(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.PresumedDeceased));
        assertEq(reg.dateOfDeath(MEMBER), lastStrong, "dated at the last identity proof");
        // If the member was in fact alive, an identity proof within 5 years brings them back.
        vm.warp(block.timestamp + 1 days);
        bytes memory wrongKey = _attest(MEMBER, 12161946, IDENTIFY, ATTESTER);
        vm.expectRevert(LifeRegistry.BadSignature.selector);
        reg.revive(MEMBER, address(idv), wrongKey);
        reg.revive(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Active));
        assertTrue(reg.canReceiveIncome(MEMBER));
        assertEq(reg.revivedAt(MEMBER), block.timestamp);
    }

    function test_deathsOlderThanFiveYearsStayFinal() public {
        // An unidentified member can't be presumed on the identity path (they have no money in).
        _enroll(2, 0xB0B);
        vm.warp(block.timestamp + 850 days);
        reg.checkIn(2, _assertion(reg.challenge(2), 0xB0B));
        vm.expectRevert(bytes("not lapsed long enough"));
        reg.presumeDeceased(2);
        reg.presumeDeceased(MEMBER);
        vm.warp(block.timestamp + 5 * 365 days + 1);
        bytes memory late = _attest(MEMBER, KEY, IDENTIFY, ATTESTER);
        vm.expectRevert(LifeRegistry.NotAllowed.selector);
        reg.revive(MEMBER, address(idv), late);
    }

    function test_thePayoutAddressMovesTheAccountAndNobodyElseCan() public {
        address newPayout = makeAddr("new wallet");
        address newHeir = makeAddr("son");
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        reg.setPayout(MEMBER, reporter); // a stranger
        vm.startPrank(payoutAddr);
        reg.setBeneficiary(MEMBER, newHeir);
        reg.setGuardians(MEMBER, [g2, g3, address(0)]);
        reg.setPayout(MEMBER, newPayout);
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        reg.setPayout(MEMBER, payoutAddr); // the old address has handed over
        vm.stopPrank();
        assertEq(reg.payoutOf(MEMBER), newPayout);
        assertEq(reg.beneficiaryOf(MEMBER), newHeir);
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        vm.prank(g1);
        reg.attest(MEMBER); // no longer a guardian
    }

    /// A member who lost her phone: the operator checks her identity again and signs a statement
    /// bound to her new wallet and new passkey. Nothing else can move the account.
    function test_recoveryNeedsAStatementBoundToTheNewWalletAndPasskey() public {
        address newPayout = makeAddr("new wallet");
        uint256 newPasskey = 0xC0FFEE;
        (uint256 qx, uint256 qy) = vm.publicKeyP256(newPasskey);
        vm.warp(block.timestamp + 10 days);
        bytes memory plain = _attest(MEMBER, KEY, IDENTIFY, ATTESTER);
        vm.expectRevert(LifeRegistry.BadSignature.selector); // an identification isn't a recovery
        reg.recover(MEMBER, newPayout, bytes32(qx), bytes32(qy), address(idv), plain);
        bytes memory other = _attest(MEMBER, KEY, reg.recoveryAction(reporter, bytes32(qx), bytes32(qy)), ATTESTER);
        vm.expectRevert(LifeRegistry.BadSignature.selector); // bound to another wallet
        reg.recover(MEMBER, newPayout, bytes32(qx), bytes32(qy), address(idv), other);
        _recover(newPayout, bytes32(qx), bytes32(qy));
        assertEq(reg.payoutOf(MEMBER), newPayout);
        LifeRegistry.WebAuthnAuth memory old = _assertion(reg.challenge(MEMBER), PASSKEY);
        vm.expectRevert(LifeRegistry.BadSignature.selector); // the lost phone's passkey is void
        reg.checkIn(MEMBER, old);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), newPasskey));
        assertTrue(reg.canReceiveIncome(MEMBER));
    }

    /// A recovery requested and, the member not having cancelled it, applied after the delay.
    function _recover(address payout, bytes32 qx, bytes32 qy) internal {
        bytes memory proof = _attest(MEMBER, KEY, reg.recoveryAction(payout, qx, qy), ATTESTER);
        reg.recover(MEMBER, payout, qx, qy, address(idv), proof);
        vm.expectRevert(bytes("waiting"));
        reg.finishRecovery(MEMBER);
        vm.warp(block.timestamp + reg.challengeWindow());
        reg.finishRecovery(MEMBER);
    }

    /// Skeptic 2: the attester's key alone took an account over at once. Now a recovery waits the
    /// challenge window, and one check-in with the member's own passkey cancels it.
    function test_anIdentityStatementAloneCannotTakeALivingMembersAccount() public {
        address thief = makeAddr("thief");
        (uint256 qx, uint256 qy) = vm.publicKeyP256(0xBAD);
        vm.warp(block.timestamp + 1 days);
        reg.recover(MEMBER, thief, bytes32(qx), bytes32(qy), address(idv), _attest(MEMBER, KEY, reg.recoveryAction(thief, bytes32(qx), bytes32(qy)), ATTESTER));
        assertEq(reg.payoutOf(MEMBER), payoutAddr, "nothing changes at once");
        vm.warp(block.timestamp + 10 days);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY)); // she is alive and has her phone
        vm.warp(block.timestamp + 150 days);
        vm.expectRevert(bytes("none"));
        reg.finishRecovery(MEMBER);
        assertEq(reg.payoutOf(MEMBER), payoutAddr);
    }

    function _readyAt() internal view returns (uint64 r) {
        (,,, r,,) = reg.pendingRecovery(MEMBER);
    }

    function _requestRecovery(address to, uint256 passkey) internal returns (uint64 at) {
        (uint256 x, uint256 y) = vm.publicKeyP256(passkey);
        (bytes32 qx, bytes32 qy) = (bytes32(x), bytes32(y));
        bytes memory proof = _attest(MEMBER, KEY, reg.recoveryAction(to, qx, qy), ATTESTER);
        at = uint64(block.timestamp);
        reg.recover(MEMBER, to, qx, qy, address(idv), proof);
    }

    /// Skeptic 3: with a 14-day delay, a recovery requested the day after her check-in completed
    /// long before her next one. It now waits the challenge window (a check-in period plus grace at
    /// least), so her usual check-in always comes first, and income waits meanwhile.
    function test_aMemberOnHerUsualScheduleAlwaysCancelsARecoveryInTime() public {
        address thief = makeAddr("thief");
        vm.warp(block.timestamp + 80 days);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        vm.warp(block.timestamp + 1 days);
        uint64 at = _requestRecovery(thief, 0xBAD);
        uint64 readyAt = _readyAt();
        assertEq(readyAt, at + reg.challengeWindow(), "the whole challenge window");
        assertGe(reg.challengeWindow(), reg.checkInPeriod() + reg.grace());
        assertFalse(reg.canReceiveIncome(MEMBER), "income waits while a recovery is pending");
        vm.warp(at + reg.challengeWindow() - 1);
        vm.expectRevert(bytes("waiting"));
        reg.finishRecovery(MEMBER);
        vm.warp(at + reg.checkInPeriod()); // her next check-in, on schedule
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        assertTrue(reg.canReceiveIncome(MEMBER));
        vm.warp(at + reg.challengeWindow() + 1);
        vm.expectRevert(bytes("none"));
        reg.finishRecovery(MEMBER);
        assertEq(reg.payoutOf(MEMBER), payoutAddr);
    }

    /// Skeptic 3: a request counted as an identity proof even when cancelled, so each attempt by a
    /// member whose phone was stolen reopened income to the thief. Now it counts only once applied.
    function test_aCancelledRecoveryIsNotAnIdentityProof() public {
        vm.warp(block.timestamp + 420 days); // her last identity proof is over 400 days old
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        assertFalse(reg.canReceiveIncome(MEMBER), "held: identity lapsed");
        uint64 strongBefore = reg.lastStrong(MEMBER);
        _requestRecovery(makeAddr("new wallet"), 0xC0FFEE);
        assertEq(reg.lastStrong(MEMBER), strongBefore, "a request is not a proof");
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY)); // whoever holds the phone cancels
        assertFalse(reg.canReceiveIncome(MEMBER), "and income stays held");
        _sameRecoveryStatementTwiceIsRefused();
    }

    function _sameRecoveryStatementTwiceIsRefused() internal {
        (uint256 qx, uint256 qy) = vm.publicKeyP256(0xC0FFEE);
        address w = makeAddr("new wallet");
        bytes32 action = reg.recoveryAction(w, bytes32(qx), bytes32(qy));
        bytes memory again = _attestAt(MEMBER, KEY, action, reg.lastRecoveryIssued(MEMBER), uint64(block.timestamp + 7 days), ATTESTER);
        vm.expectRevert(LifeRegistry.BadSignature.selector);
        reg.recover(MEMBER, w, bytes32(qx), bytes32(qy), address(idv), again);
    }

    /// An applied recovery counts as her identity proof (the statement was issued for her).
    function test_anAppliedRecoveryRenewsIdentity() public {
        vm.warp(block.timestamp + 420 days);
        address w = makeAddr("new wallet");
        (uint256 qx, uint256 qy) = vm.publicKeyP256(0xC0FFEE);
        _recover(w, bytes32(qx), bytes32(qy));
        assertTrue(reg.canReceiveIncome(MEMBER));
    }

    /// Skeptic 3 (mutation survivors): an identity statement answers a report; a recovery request
    /// doesn't, and a death made final cancels the pending recovery.
    function test_anIdentityStatementAnswersAReportARecoveryRequestDoesNot() public {
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("forged certificate"));
        vm.warp(block.timestamp + 10 days);
        reg.strongProof(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        reg.resolveReport(MEMBER);
        assertEq(usdg.balanceOf(payoutAddr), 10e6, "answered by her identity proof");

        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("certificate"));
        vm.warp(block.timestamp + 1 days);
        _requestRecovery(makeAddr("new wallet"), 0xC0FFEE);
        vm.warp(block.timestamp + reg.challengeWindow());
        reg.resolveReport(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Deceased), "a request is not her proof of life");
        uint64 readyAt = _readyAt();
        assertEq(readyAt, 0, "the death cancels the recovery");
    }

    /// Skeptic 3: a presumption must also clear an earlier revival, and close a report still open
    /// (returning its bond) instead of letting it rewrite the death later.
    function test_aPresumptionClearsAnEarlierRevivalAndClosesAnOpenReport() public {
        vm.warp(block.timestamp + reg.presumption() + 1 days);
        reg.presumeDeceased(MEMBER);
        vm.warp(block.timestamp + 1 days);
        reg.revive(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        assertGt(reg.revivedAt(MEMBER), 0);
        vm.warp(block.timestamp + 10 days);
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("certificate"));
        vm.warp(block.timestamp + reg.presumption() + 1 days);
        reg.presumeDeceased(MEMBER);
        assertEq(reg.revivedAt(MEMBER), 0, "no stale revival");
        assertEq(usdg.balanceOf(reporter), 100e6, "the open report's bond is returned");
        vm.expectRevert(bytes("no report"));
        reg.resolveReport(MEMBER);
    }

    /// Skeptic 4: a member who relies on guardians never checks in herself, so only her payout
    /// address (her family's wallet) can stop a misused identity statement. Nobody else can.
    function test_thePayoutAddressCanCancelARecoveryAndNobodyElse() public {
        vm.warp(block.timestamp + 1 days); // statements must be newer than set-up's
        _requestRecovery(makeAddr("thief"), 0xBAD);
        vm.prank(g1);
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        reg.cancelRecovery(MEMBER);
        vm.prank(payoutAddr);
        reg.cancelRecovery(MEMBER);
        vm.warp(block.timestamp + reg.challengeWindow() + 1);
        vm.expectRevert(bytes("none"));
        reg.finishRecovery(MEMBER);
    }

    /// Skeptic 4: a newer request must wait its own window, not inherit the first one's.
    function test_aNewRecoveryRequestRestartsTheWindow() public {
        vm.warp(block.timestamp + 1 days); // statements must be newer than set-up's
        _requestRecovery(makeAddr("first"), 0xBAD);
        vm.warp(block.timestamp + 60 days);
        uint64 at = _requestRecovery(makeAddr("second"), 0xC0FFEE);
        assertEq(_readyAt(), at + reg.challengeWindow());
    }

    /// Skeptic 4: once applied, a recovery is the member's own proof of life (it can answer a
    /// report), and the pending one is gone.
    function test_anAppliedRecoveryIsHerOwnProofOfLife() public {
        vm.warp(block.timestamp + 1 days); // statements must be newer than set-up's
        (uint256 qx, uint256 qy) = vm.publicKeyP256(0xC0FFEE);
        _recover(makeAddr("new wallet"), bytes32(qx), bytes32(qy));
        assertEq(reg.lastOwnProof(MEMBER), block.timestamp);
        assertEq(_readyAt(), 0);
    }

    /// Skeptic 4: a member whose identity lapsed and who asked for a recovery was presumed dead,
    /// dated two years back, while the recovery waited. Not on that path any more.
    function test_aMemberMidRecoveryIsNotPresumedForALapsedIdentity() public {
        for (uint256 i; i < 10; i++) {
            vm.warp(block.timestamp + 80 days);
            reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY)); // alive, checking in
        }
        _requestRecovery(makeAddr("new wallet"), 0xC0FFEE); // day 800: identity 800 days old
        vm.warp(block.timestamp + 10 days);
        vm.expectRevert(bytes("not lapsed long enough"));
        reg.presumeDeceased(MEMBER);
    }

    /// Skeptic 4 (a surviving mutation): after two years of complete silence she is presumed dead
    /// even with a recovery pending, and the recovery goes with her; and a presumption, like a
    /// revival, leaves no reporter on record.
    function test_aSilentPresumptionClearsAPendingRecoveryAndAnyReporter() public {
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("forged certificate"));
        vm.warp(block.timestamp + 120 days);
        reg.resolveReport(MEMBER);
        assertEq(reg.reporterOf(MEMBER), reporter);
        vm.warp(block.timestamp + 1 days);
        reg.revive(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        assertEq(reg.reporterOf(MEMBER), address(0), "revived: no reporter on record");
        vm.warp(block.timestamp + 700 days);
        _requestRecovery(makeAddr("new wallet"), 0xC0FFEE);
        vm.warp(block.timestamp + 31 days);
        reg.presumeDeceased(MEMBER);
        assertEq(_readyAt(), 0, "the pending recovery goes with the presumption");
        assertEq(reg.reporterOf(MEMBER), address(0));
    }

    /// Skeptic 3: one address listed three times counted as 2 of 3 guardians.
    function test_guardiansMustBeDifferentPeople() public {
        vm.prank(payoutAddr);
        vm.expectRevert(bytes("guardians"));
        reg.setGuardians(MEMBER, [g1, g1, g2]);
        vm.prank(payoutAddr);
        vm.expectRevert(bytes("guardians"));
        reg.setGuardians(MEMBER, [g1, g2, g1]); // not only neighbours (skeptic 4)
        vm.prank(payoutAddr);
        reg.setGuardians(MEMBER, [g1, address(0), address(0)]); // empty slots are fine
    }

    /// Skeptic 2: two guardians (the member's own choice) could answer an honest death report and
    /// take the reporter's bond. Guardians keep a member from lapsing; only the member answers.
    function test_guardiansCannotAnswerADeathReport() public {
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("certificate"));
        vm.warp(block.timestamp + 10 days);
        vm.prank(g1);
        reg.attest(MEMBER);
        vm.prank(g2);
        reg.attest(MEMBER);
        vm.expectRevert(bytes("window open")); // not rejected by the guardians
        reg.resolveReport(MEMBER);
        vm.warp(block.timestamp + 110 days);
        reg.resolveReport(MEMBER);
        assertEq(uint8(reg.status(MEMBER)), uint8(LifeRegistry.Status.Deceased));
        assertEq(usdg.balanceOf(payoutAddr), 0, "the family doesn't get the honest reporter's bond");
    }

    /// Skeptic 2: a reporter could date the death at 0 and cut the estate's income. Nobody died
    /// before their last proof of life (skeptic 3: their guardians' counts too).
    function test_aReportCannotDateTheDeathBeforeTheLastProofOfLife() public {
        vm.warp(block.timestamp + 20 days);
        reg.checkIn(MEMBER, _assertion(reg.challenge(MEMBER), PASSKEY));
        uint64 lastSeen = uint64(block.timestamp);
        vm.warp(block.timestamp + 5 days);
        vm.prank(reporter);
        reg.reportDeath(MEMBER, 0, keccak256("certificate"));
        vm.warp(block.timestamp + 120 days);
        reg.resolveReport(MEMBER);
        assertEq(reg.dateOfDeath(MEMBER), lastSeen);
    }

    function test_aGuardianReliantMembersDeathIsDatedAtHerGuardiansLastProof() public {
        vm.warp(block.timestamp + 100 days);
        vm.prank(g1);
        reg.attest(MEMBER);
        vm.prank(g2);
        reg.attest(MEMBER);
        uint64 lastSeen = uint64(block.timestamp);
        vm.warp(block.timestamp + 5 days);
        vm.prank(reporter);
        reg.reportDeath(MEMBER, 0, keccak256("certificate"));
        vm.warp(block.timestamp + 120 days);
        reg.resolveReport(MEMBER);
        assertEq(reg.dateOfDeath(MEMBER), lastSeen);
    }

    /// Skeptic 2: a revival stayed on record, so after a later, real death the pool would have repaid
    /// the dead member from its reserve. Every final death clears it.
    function test_aLaterDeathClearsAnEarlierRevival() public {
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("forged certificate"));
        vm.warp(block.timestamp + 120 days);
        reg.resolveReport(MEMBER);
        vm.warp(block.timestamp + 1 days);
        reg.revive(MEMBER, address(idv), _attest(MEMBER, KEY, IDENTIFY, ATTESTER));
        assertGt(reg.revivedAt(MEMBER), 0);
        vm.warp(block.timestamp + 3 * 365 days);
        vm.prank(reporter);
        reg.reportDeath(MEMBER, uint64(block.timestamp), keccak256("certificate"));
        vm.warp(block.timestamp + 120 days);
        reg.resolveReport(MEMBER);
        assertEq(reg.revivedAt(MEMBER), 0, "no stale revival to repay a real death");
    }

    function test_aVerifierAddedAfterLaunchWaitsThirtyDays() public {
        AttestedIdentity second = new AttestedIdentity(vm.addr(0xBEEF), address(reg));
        reg.setVerifier(address(second), true);
        _enroll(2, 0xB0B);
        uint64 t = uint64(block.timestamp);
        bytes memory proof = _attestVia(address(second), 2, KEY, IDENTIFY, t, t + 7 days, 0xBEEF);
        vm.expectRevert(LifeRegistry.NotAllowed.selector);
        reg.strongProof(2, address(second), proof);
        vm.warp(block.timestamp + 30 days);
        t = uint64(block.timestamp);
        reg.strongProof(2, address(second), _attestVia(address(second), 2, KEY, IDENTIFY, t, t + 7 days, 0xBEEF));
        assertTrue(reg.life(2).identified);
        // Removal is immediate.
        reg.setVerifier(address(second), false);
        vm.warp(block.timestamp + 1 days);
        t = uint64(block.timestamp);
        bytes memory again = _attestVia(address(second), 2, KEY, IDENTIFY, t, t + 7 days, 0xBEEF);
        vm.expectRevert(LifeRegistry.NotAllowed.selector);
        reg.strongProof(2, address(second), again);
    }

    function test_governanceBoundsAndPoolOnlyEnrollment() public {
        vm.expectRevert(bytes("checkIn"));
        reg.setPeriods(1 days, 30 days, 730 days, 120 days);
        vm.expectRevert(bytes("window")); // shorter than a check-in period plus grace
        reg.setPeriods(90 days, 30 days, 730 days, 119 days);
        reg.setPeriods(60 days, 30 days, 730 days, 90 days);
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        reg.enroll(2, KEY, bytes32(0), bytes32(0), address(1), address(1), [address(0), address(0), address(0)]);
    }
}
