// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {IERC20Min, IStockToken, IERC4626Min, AggregatorV3, PoolKey, SwapParams, IPoolManager, IUnlockCallback} from "./interfaces/External.sol";

/// @title Tonti Treasury
/// @notice Holds the pool's assets in three sleeves and executes every trade the pool's ledger orders:
///   0 = cash:  USDG in the Morpho Steakhouse USDG vault (shares held here)
///   1 = SGOV:  Robinhood SGOV stock token (T-bills)
///   2 = SPY:   Robinhood SPY stock token
/// Accounting lives in the pool (Stylus); this contract only holds, prices and trades.
/// @dev docs/protocol.md §8. Prices come from Chainlink and are refused when stale or when the token
/// is paused. Swaps go straight through the v4 PoolManager (unlock callback), not Robinhood's modified
/// UniversalRouter, and revert if the fill is more than `maxSlippageBps` worse than Chainlink.
contract Treasury is IUnlockCallback {
    uint256 internal constant WAD = 1e18;
    uint256 internal constant SLEEVES = 3;
    uint160 internal constant MIN_SQRT_PRICE_PLUS_1 = 4295128740;
    uint160 internal constant MAX_SQRT_PRICE_MINUS_1 = 1461446703485210103287273052203988822378723970341;

    IERC20Min public immutable usdg;
    IERC4626Min public immutable cashVault;
    IStockToken public immutable sgov;
    IStockToken public immutable spy;
    AggregatorV3 public immutable sgovFeed;
    AggregatorV3 public immutable spyFeed;
    IPoolManager public immutable poolManager;

    /// The ledger contract allowed to move funds.
    address public pool;
    address public owner;
    /// Oracle freshness: settlement only runs while feeds are fresh (US market hours in practice).
    uint256 public maxStaleness = 26 hours;
    uint256 public maxSlippageBps = 50;
    PoolKey internal sgovKey;
    PoolKey internal spyKey;

    error Unauthorized();
    error StalePrice(uint8 sleeve, uint256 updatedAt);
    error BadPrice(uint8 sleeve);
    error Paused(uint8 sleeve);
    error Slippage(uint8 sleeve, uint256 got, uint256 minOut);
    error BadSleeve();

    event PoolSet(address pool);
    event Traded(uint8 indexed sleeve, bool buy, uint256 amountIn, uint256 amountOut);
    event Paid(address indexed to, uint256 usdg);
    event RouteSet(uint8 indexed sleeve, PoolKey key);

    struct SwapOrder {
        PoolKey key;
        bool zeroForOne;
        uint256 amountIn;
        address tokenIn;
        address tokenOut;
    }

    modifier onlyPool() {
        if (msg.sender != pool) revert Unauthorized();
        _;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert Unauthorized();
        _;
    }

    constructor(
        address usdg_,
        address cashVault_,
        address sgov_,
        address spy_,
        address sgovFeed_,
        address spyFeed_,
        address poolManager_,
        PoolKey memory sgovKey_,
        PoolKey memory spyKey_
    ) {
        usdg = IERC20Min(usdg_);
        cashVault = IERC4626Min(cashVault_);
        sgov = IStockToken(sgov_);
        spy = IStockToken(spy_);
        sgovFeed = AggregatorV3(sgovFeed_);
        spyFeed = AggregatorV3(spyFeed_);
        poolManager = IPoolManager(poolManager_);
        sgovKey = sgovKey_;
        spyKey = spyKey_;
        owner = msg.sender;
        require(cashVault.asset() == usdg_, "vault asset");
    }

    /// @notice One-time wiring to the ledger; ownership then moves to the timelock.
    function setPool(address pool_) external onlyOwner {
        require(pool == address(0), "pool set");
        pool = pool_;
        emit PoolSet(pool_);
    }

    function transferOwnership(address to) external onlyOwner {
        owner = to;
    }

    /// @notice Bounded: at least 1 hour, at most 4 days (a long weekend plus a holiday).
    /// @notice Re-routes a sleeve's swaps, e.g. if its Uniswap v4 pool loses its liquidity; otherwise
    /// every settlement that sells the sleeve would revert. Owner only, so behind the 48-hour
    /// timelock. Safe to allow: the key must pair USDG with the sleeve's own token and have no hooks,
    /// and every fill is still bounded against Chainlink by `maxSlippageBps`.
    function setRoute(uint8 sleeve, PoolKey calldata key) external onlyOwner {
        if (sleeve != 1 && sleeve != 2) revert BadSleeve();
        address token = sleeve == 1 ? address(sgov) : address(spy);
        (address c0, address c1) = address(usdg) < token ? (address(usdg), token) : (token, address(usdg));
        require(key.currency0 == c0 && key.currency1 == c1, "pair");
        require(key.hooks == address(0), "hooks");
        if (sleeve == 1) sgovKey = key;
        else spyKey = key;
        emit RouteSet(sleeve, key);
    }

    /// @notice The Uniswap v4 pool a sleeve trades through.
    function route(uint8 sleeve) external view returns (PoolKey memory) {
        if (sleeve != 1 && sleeve != 2) revert BadSleeve();
        return sleeve == 1 ? sgovKey : spyKey;
    }

    function setMaxStaleness(uint256 seconds_) external onlyOwner {
        // At least 26 hours (skeptic review 4): feeds update daily, and a shorter limit would stop
        // every settlement.
        require(seconds_ >= 26 hours && seconds_ <= 96 hours, "bounds");
        maxStaleness = seconds_;
    }

    /// @notice Bounded: at most 2%.
    function setMaxSlippageBps(uint256 bps) external onlyOwner {
        // At least 0.3% (skeptic review 4): measured fills sit up to ~0.13% from the oracle, so a
        // tighter bound would make every sale revert and stop income.
        require(bps >= 30 && bps <= 200, "bounds");
        maxSlippageBps = bps;
    }

    // ------------------------------------------------------------------ prices

    /// @notice USDG per whole share of each sleeve, as WAD. Reverts on a stale feed or a paused token.
    function prices() public view returns (uint256[SLEEVES] memory p) {
        // Cash: one vault share's USDG value (USDG has 6 decimals → scale to WAD).
        p[0] = cashVault.convertToAssets(WAD) * 1e12;
        p[1] = _feedPrice(1, sgov, sgovFeed);
        p[2] = _feedPrice(2, spy, spyFeed);
    }

    function _feedPrice(uint8 sleeve, IStockToken token, AggregatorV3 feed) internal view returns (uint256) {
        if (token.paused()) revert Paused(sleeve);
        (, int256 answer,, uint256 updatedAt,) = feed.latestRoundData();
        if (answer <= 0) revert BadPrice(sleeve);
        if (block.timestamp - updatedAt > maxStaleness) revert StalePrice(sleeve, updatedAt);
        // Chainlink prices include the ERC-8056 multiplier already; do not apply it again.
        return uint256(answer) * 10 ** (18 - feed.decimals());
    }

    /// @notice The latest oracle prices without the staleness check, and when the oldest feed last
    /// updated: for showing members what they hold while markets are closed. Never used to trade or
    /// to price deposits (those use `prices`).
    function lastPrices() external view returns (uint256[SLEEVES] memory p, uint256 oldestUpdate) {
        p[0] = cashVault.convertToAssets(WAD) * 1e12;
        oldestUpdate = block.timestamp;
        (p[1], oldestUpdate) = _lastFeed(1, sgovFeed, oldestUpdate);
        (p[2], oldestUpdate) = _lastFeed(2, spyFeed, oldestUpdate);
    }

    function _lastFeed(uint8 sleeve, AggregatorV3 feed, uint256 oldest) internal view returns (uint256, uint256) {
        (, int256 answer,, uint256 updatedAt,) = feed.latestRoundData();
        if (answer <= 0) revert BadPrice(sleeve);
        return (uint256(answer) * 10 ** (18 - feed.decimals()), updatedAt < oldest ? updatedAt : oldest);
    }

    /// @notice Shares of each sleeve held.
    function holdings() external view returns (uint256[SLEEVES] memory h) {
        h[0] = cashVault.balanceOf(address(this));
        h[1] = sgov.balanceOf(address(this));
        h[2] = spy.balanceOf(address(this));
    }

    // ------------------------------------------------------------------ pool operations

    /// @notice Put USDG already sent here into the cash sleeve. Returns vault shares minted.
    function depositCash(uint256 usdgAmount) external onlyPool returns (uint256 shares) {
        usdg.approve(address(cashVault), usdgAmount);
        shares = cashVault.deposit(usdgAmount, address(this));
    }

    /// @notice Convert sleeve shares to USDG (kept here for income payouts). Returns USDG received.
    function sell(uint256[SLEEVES] calldata shares) external onlyPool returns (uint256 usdgOut) {
        uint256[SLEEVES] memory p = prices();
        if (shares[0] > 0) usdgOut += cashVault.redeem(shares[0], address(this), address(this));
        if (shares[1] > 0) usdgOut += _trade(1, false, shares[1], p[1]);
        if (shares[2] > 0) usdgOut += _trade(2, false, shares[2], p[2]);
    }

    /// @notice Buy a sleeve with USDG held here. Returns shares received.
    function buy(uint8 sleeve, uint256 usdgIn) external onlyPool returns (uint256 sharesOut) {
        uint256[SLEEVES] memory p = prices();
        if (sleeve == 0) {
            usdg.approve(address(cashVault), usdgIn);
            return cashVault.deposit(usdgIn, address(this));
        }
        if (sleeve > 2) revert BadSleeve();
        sharesOut = _trade(sleeve, true, usdgIn, p[sleeve]);
    }

    /// @notice Pay income or a claim in USDG.
    function pay(address to, uint256 usdgAmount) external onlyPool {
        require(usdg.transfer(to, usdgAmount), "transfer");
        emit Paid(to, usdgAmount);
    }

    // ------------------------------------------------------------------ swaps

    /// @dev buy: USDG → token; sell: token → USDG. Enforces the fill against Chainlink.
    function _trade(uint8 sleeve, bool isBuy, uint256 amountIn, uint256 priceWad) internal returns (uint256 out) {
        IStockToken token = sleeve == 1 ? sgov : spy;
        PoolKey memory key = sleeve == 1 ? sgovKey : spyKey;
        address tokenIn = isBuy ? address(usdg) : address(token);
        address tokenOut = isBuy ? address(token) : address(usdg);
        bool zeroForOne = tokenIn == key.currency0;
        out = abi.decode(
            poolManager.unlock(abi.encode(SwapOrder(key, zeroForOne, amountIn, tokenIn, tokenOut))), (uint256)
        );
        // Oracle-implied output: USDG has 6 decimals, stock tokens 18, price is WAD USDG per token.
        uint256 fair = isBuy ? amountIn * 1e12 * WAD / priceWad : amountIn * priceWad / WAD / 1e12;
        uint256 minOut = fair * (10_000 - maxSlippageBps) / 10_000;
        if (out < minOut) revert Slippage(sleeve, out, minOut);
        emit Traded(sleeve, isBuy, amountIn, out);
    }

    /// @inheritdoc IUnlockCallback
    function unlockCallback(bytes calldata data) external returns (bytes memory) {
        if (msg.sender != address(poolManager)) revert Unauthorized();
        SwapOrder memory o = abi.decode(data, (SwapOrder));
        int256 delta = poolManager.swap(
            o.key,
            SwapParams({
                zeroForOne: o.zeroForOne,
                amountSpecified: -int256(o.amountIn),
                sqrtPriceLimitX96: o.zeroForOne ? MIN_SQRT_PRICE_PLUS_1 : MAX_SQRT_PRICE_MINUS_1
            }),
            ""
        );
        // BalanceDelta packs amount0 (high 128 bits) and amount1 (low 128 bits), from our side.
        int128 d0 = int128(delta >> 128);
        int128 d1 = int128(delta);
        (int128 owed, int128 received) = o.zeroForOne ? (d0, d1) : (d1, d0);
        uint256 pay_ = uint256(uint128(-owed));
        uint256 got = uint256(uint128(received));
        poolManager.sync(o.tokenIn);
        require(IERC20Min(o.tokenIn).transfer(address(poolManager), pay_), "settle transfer");
        poolManager.settle();
        poolManager.take(o.tokenOut, address(this), got);
        return abi.encode(got);
    }
}
