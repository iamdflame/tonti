// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import {Test} from "forge-std/Test.sol";
import {TimelockController} from "@openzeppelin/contracts/governance/TimelockController.sol";
import {LifeRegistry} from "../src/LifeRegistry.sol";

/// deploy/deploy.sh hands every contract to a 48-hour TimelockController. After that, nobody
/// changes a parameter without 48 hours' public notice, and the contracts' own bounds still apply.
contract GovernanceTest is Test {
    address constant PROPOSER = address(0xA11CE);
    LifeRegistry registry;
    TimelockController timelock;

    function setUp() public {
        registry = new LifeRegistry(address(0xBEEF));
        address[] memory who = new address[](1);
        who[0] = PROPOSER;
        // As deployed: the deployer proposes (and can cancel); anyone may execute (address(0)).
        address[] memory anyone = new address[](1);
        timelock = new TimelockController(48 hours, who, anyone, address(0));
        registry.transferOwnership(address(timelock));
    }

    function test_the_deployer_loses_direct_control() public {
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        registry.setPeriods(60 days, 30 days, 730 days, 30 days);
        vm.expectRevert(LifeRegistry.Unauthorized.selector);
        registry.setVerifier(address(0xBAD), true);
    }

    function test_changes_take_effect_only_after_48_hours_notice() public {
        bytes memory data = abi.encodeCall(LifeRegistry.setPeriods, (60 days, 30 days, 730 days, 90 days));
        vm.startPrank(PROPOSER);
        // A shorter notice can't even be scheduled.
        vm.expectRevert();
        timelock.schedule(address(registry), 0, data, bytes32(0), bytes32(0), 1 hours);
        timelock.schedule(address(registry), 0, data, bytes32(0), bytes32(0), 48 hours);
        vm.warp(block.timestamp + 48 hours - 1);
        vm.expectRevert();
        timelock.execute(address(registry), 0, data, bytes32(0), bytes32(0));
        vm.stopPrank();
        // Once the notice has run, anyone may execute it: the proposer can't sit on a queued change.
        vm.warp(block.timestamp + 1);
        vm.prank(address(0xCAFE));
        timelock.execute(address(registry), 0, data, bytes32(0), bytes32(0));
        assertEq(registry.checkInPeriod(), 60 days);
    }

    function test_even_the_timelock_is_bounded_by_the_contract() public {
        // A 1-day check-in period is outside the contract's 30–180 day bounds.
        bytes memory data = abi.encodeCall(LifeRegistry.setPeriods, (1 days, 30 days, 730 days, 120 days));
        vm.startPrank(PROPOSER);
        timelock.schedule(address(registry), 0, data, bytes32(0), bytes32(0), 48 hours);
        vm.warp(block.timestamp + 48 hours);
        vm.expectRevert();
        timelock.execute(address(registry), 0, data, bytes32(0), bytes32(0));
        vm.stopPrank();
        assertEq(registry.checkInPeriod(), 90 days);
    }
}
