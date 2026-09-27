// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {Treasury} from "../src/Treasury.sol";
import {IERC20Min, IStockToken, PoolKey, AggregatorV3} from "../src/interfaces/External.sol";

/// Runs against real Robinhood Chain mainnet state (real USDG, SGOV, SPY, Chainlink feeds, the Morpho
/// Steakhouse vault and the Uniswap v4 PoolManager).
/// The public RPC is not an archive node (100 ms blocks, pruned state), so it forks the latest block;
/// set FORK_BLOCK together with an archive ROBINHOOD_RPC to pin a block.
///   forge test --match-contract TreasuryFork -vv
contract TreasuryForkTest is Test {
    address constant USDG = 0x5fc5360D0400a0Fd4f2af552ADD042D716F1d168;
    address constant STEAK_USDG = 0xBeEff033F34C046626B8D0A041844C5d1A5409dd;
    address constant SGOV = 0x92FD66527192E3e61d4DDd13322Aa222DE86F9B5;
    address constant SPY = 0x117cc2133c37B721F49dE2A7a74833232B3B4C0C;
    address constant SGOV_FEED = 0xa0DF4ee0fFf975306345875E3548Fcc519577A11;
    address constant SPY_FEED = 0x319724394D3A0e3669269846abE664Cd621f9f6A;
    address constant POOL_MANAGER = 0x8366a39CC670B4001A1121B8F6A443A643e40951;

    Treasury treasury;
    address ledger = makeAddr("ledger");

    function setUp() public {
        uint256 pinned = vm.envOr("FORK_BLOCK", uint256(0));
        string memory rpc = vm.envOr("ROBINHOOD_RPC", string("robinhood"));
        if (pinned == 0) vm.createSelectFork(rpc);
        else vm.createSelectFork(rpc, pinned);
        // Measured pools: USDG/SGOV 0.0375% (ts 4), SPY/USDG 0.05% (ts 5), no hooks.
        PoolKey memory sgovKey = PoolKey(USDG, SGOV, 375, 4, address(0));
        PoolKey memory spyKey = PoolKey(SPY, USDG, 500, 5, address(0));
        treasury = new Treasury(USDG, STEAK_USDG, SGOV, SPY, SGOV_FEED, SPY_FEED, POOL_MANAGER, sgovKey, spyKey);
        treasury.setPool(ledger);
        deal(USDG, address(treasury), 20_000e6);
    }

    function test_staleFeedGuardMatchesFeedAge() public {
        // Whatever the fork time, prices() must refuse exactly when a stock feed is older than 26 h.
        (,,, uint256 spyAt,) = AggregatorV3(SPY_FEED).latestRoundData();
        (,,, uint256 sgovAt,) = AggregatorV3(SGOV_FEED).latestRoundData();
        uint256 oldest = spyAt < sgovAt ? spyAt : sgovAt;
        emit log_named_uint("oldest feed age (h)", (block.timestamp - oldest) / 1 hours);
        if (block.timestamp - oldest > 26 hours) {
            vm.expectRevert();
            treasury.prices();
        } else {
            treasury.prices();
        }
    }

    function test_pricesMatchChainlinkWhenAllowed() public {
        treasury.setMaxStaleness(96 hours);
        uint256[3] memory p = treasury.prices();
        (, int256 sgovAns,,,) = AggregatorV3(SGOV_FEED).latestRoundData();
        (, int256 spyAns,,,) = AggregatorV3(SPY_FEED).latestRoundData();
        assertApproxEqRel(p[0], 1.008e18, 0.01e18, "steakUSDG share ~1.008 USDG");
        assertEq(p[1], uint256(sgovAns) * 1e10, "SGOV = Chainlink, scaled to WAD");
        assertEq(p[2], uint256(spyAns) * 1e10, "SPY = Chainlink, scaled to WAD");
    }

    /// Members can always see what they hold: `lastPrices` answers even when the feeds are too old
    /// to trade on (weekends), with the same prices and the age of the oldest feed.
    function test_lastPricesAnswerEvenWhenFeedsAreTooOldToTrade() public {
        (uint256[3] memory p, uint256 oldest) = treasury.lastPrices();
        (, int256 sgovAns,, uint256 sgovAt,) = AggregatorV3(SGOV_FEED).latestRoundData();
        (, int256 spyAns,, uint256 spyAt,) = AggregatorV3(SPY_FEED).latestRoundData();
        assertEq(p[1], uint256(sgovAns) * 1e10);
        assertEq(p[2], uint256(spyAns) * 1e10);
        assertEq(oldest, sgovAt < spyAt ? sgovAt : spyAt, "the older feed's time");
        vm.warp(block.timestamp + 7 days); // far past the 26-hour limit
        vm.expectRevert();
        treasury.prices();
        (uint256[3] memory later,) = treasury.lastPrices();
        assertEq(later[2], p[2], "still answers, with the same prices");
    }

    function test_routesCanBeReplacedOnlyByTheOwnerAndOnlyForTheRightPair() public {
        PoolKey memory spyKey = PoolKey(SPY, USDG, 500, 5, address(0));
        vm.prank(ledger);
        vm.expectRevert(Treasury.Unauthorized.selector);
        treasury.setRoute(2, spyKey);
        vm.expectRevert(bytes("pair"));
        treasury.setRoute(2, PoolKey(USDG, SGOV, 375, 4, address(0))); // SGOV's pool for SPY
        vm.expectRevert(bytes("hooks"));
        treasury.setRoute(2, PoolKey(SPY, USDG, 500, 5, address(0xB00C)));
        vm.expectRevert(Treasury.BadSleeve.selector);
        treasury.setRoute(0, spyKey);
        treasury.setMaxStaleness(96 hours);
        // A route through a pool nobody created fails the trade (and so the settlement)...
        treasury.setRoute(2, PoolKey(SPY, USDG, 777, 13, address(0)));
        vm.prank(ledger);
        vm.expectRevert();
        treasury.buy(2, 1_000e6);
        // ...and re-routing restores it. Mainnet has a second, 0.30% SPY/USDG pool: a real fallback,
        // and the oracle bound still holds on it.
        treasury.setRoute(2, PoolKey(SPY, USDG, 3000, 60, address(0)));
        vm.prank(ledger);
        uint256 viaFallback = treasury.buy(2, 1_000e6);
        (, int256 spyAns,,,) = AggregatorV3(SPY_FEED).latestRoundData();
        assertApproxEqRel(viaFallback, uint256(1_000e18) * 1e8 / uint256(spyAns), 0.005e18, "fallback pool fills within 50 bps");
        emit log_named_decimal_uint("SPY for $1,000 via the 0.30% pool", viaFallback, 18);
        treasury.setRoute(2, spyKey);
        vm.prank(ledger);
        assertGt(treasury.buy(2, 1_000e6), 0, "and back on the main 0.05% pool");
        assertEq(treasury.route(2).fee, 500);
    }

    function test_buyAndSellSpyWithinOracleBounds() public {
        treasury.setMaxStaleness(96 hours);
        vm.startPrank(ledger);
        uint256 spyOut = treasury.buy(2, 1_000e6);
        (, int256 spyAns,,,) = AggregatorV3(SPY_FEED).latestRoundData();
        assertApproxEqRel(spyOut, uint256(1_000e18) * 1e8 / uint256(spyAns), 0.003e18, "fill within 30 bps of Chainlink");
        uint256[3] memory sell = [uint256(0), 0, spyOut];
        uint256 back = treasury.sell(sell);
        vm.stopPrank();
        // Round trip costs two 0.05% fees plus spread: under 20 bps.
        assertGt(back, 998e6, "round trip < 20 bps");
        emit log_named_decimal_uint("SPY bought for $1,000", spyOut, 18);
        emit log_named_decimal_uint("USDG back", back, 6);
    }

    function test_buyAndSellSgov() public {
        treasury.setMaxStaleness(96 hours);
        vm.startPrank(ledger);
        uint256 sgovOut = treasury.buy(1, 1_000e6);
        uint256[3] memory sell = [uint256(0), sgovOut, 0];
        uint256 back = treasury.sell(sell);
        vm.stopPrank();
        assertGt(back, 998e6, "round trip < 20 bps");
        emit log_named_decimal_uint("SGOV bought for $1,000", sgovOut, 18);
        emit log_named_decimal_uint("USDG back", back, 6);
    }

    function test_cashSleeveDepositAndRedeem() public {
        vm.startPrank(ledger);
        uint256 shares = treasury.depositCash(1_000e6);
        uint256[3] memory sell = [shares, 0, 0];
        // Cash redemption needs no stock prices; allow the weekend by widening staleness first.
        vm.stopPrank();
        treasury.setMaxStaleness(96 hours);
        vm.prank(ledger);
        uint256 back = treasury.sell(sell);
        assertApproxEqAbs(back, 1_000e6, 2, "vault round trip is lossless to 2 raw units");
    }

    function test_slippageGuardRejectsWorseThanOracleFill() public {
        // Deterministic whatever the market does that day (skeptic 2: with the real feed, SGOV once
        // filled 0.2 bps better than a 37-hour-old price and a zero-tolerance test failed). The feed
        // is made to price SGOV 1% below the market, so the fill is ~1% worse than the oracle says
        // it should be, and the default 0.5% tolerance must refuse it.
        (uint80 id, int256 answer,,, uint80 inRound) = AggregatorV3(SGOV_FEED).latestRoundData();
        vm.mockCall(
            SGOV_FEED,
            abi.encodeWithSelector(AggregatorV3.latestRoundData.selector),
            abi.encode(id, answer * 99 / 100, block.timestamp, block.timestamp, inRound)
        );
        assertEq(treasury.maxSlippageBps(), 50);
        vm.prank(ledger);
        vm.expectRevert();
        treasury.buy(1, 1_000e6);
        // The same buy against the real (if weekend-old) feed goes through: it was the guard.
        vm.clearMockedCalls();
        treasury.setMaxStaleness(96 hours);
        vm.prank(ledger);
        treasury.buy(1, 1_000e6);
    }

    /// Skeptic 4: a 1-hour staleness limit or a 0% slippage bound would make every settlement's sale
    /// revert and stop income. Governance can't set either.
    function test_boundsThatWouldStopSettlementAreRefused() public {
        vm.expectRevert(bytes("bounds"));
        treasury.setMaxStaleness(25 hours);
        vm.expectRevert(bytes("bounds"));
        treasury.setMaxSlippageBps(29);
        treasury.setMaxStaleness(26 hours);
        treasury.setMaxSlippageBps(30);
    }

    function test_onlyLedgerMovesFunds() public {
        vm.expectRevert(Treasury.Unauthorized.selector);
        treasury.buy(2, 1e6);
        vm.expectRevert(Treasury.Unauthorized.selector);
        treasury.pay(address(this), 1e6);
    }
}
