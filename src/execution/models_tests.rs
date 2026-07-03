use super::models::{CommissionModel, SlippageModel};
use crate::events::Side;

#[test]
fn no_slippage_leaves_price_unchanged() {
    assert_eq!(
        SlippageModel::None.adjusted_price(100.0, Side::Buy, 10.0, 1_000.0),
        100.0
    );
    assert_eq!(
        SlippageModel::None.adjusted_price(100.0, Side::Sell, 10.0, 1_000.0),
        100.0
    );
}

#[test]
fn fixed_bps_moves_price_against_the_trader() {
    // 50 bps = 0.5% of 100 = 0.50.
    let model = SlippageModel::FixedBps(50.0);
    assert_eq!(model.adjusted_price(100.0, Side::Buy, 10.0, 1_000.0), 100.5);
    assert_eq!(model.adjusted_price(100.0, Side::Sell, 10.0, 1_000.0), 99.5);
}

#[test]
fn volume_impact_scales_with_participation() {
    // participation = 100 / 1,000 = 10%; impact_coefficient 2.0 -> 20% adverse move.
    let model = SlippageModel::VolumeImpact {
        participation_limit: 1.0,
        impact_coefficient: 2.0,
    };
    assert_eq!(
        model.adjusted_price(100.0, Side::Buy, 100.0, 1_000.0),
        120.0
    );
    assert_eq!(
        model.adjusted_price(100.0, Side::Sell, 100.0, 1_000.0),
        80.0
    );
}

#[test]
fn volume_impact_is_zero_when_bar_volume_is_zero() {
    let model = SlippageModel::VolumeImpact {
        participation_limit: 1.0,
        impact_coefficient: 2.0,
    };
    assert_eq!(model.adjusted_price(100.0, Side::Buy, 100.0, 0.0), 100.0);
}

#[test]
fn per_share_commission_scales_with_quantity() {
    let model = CommissionModel::PerShare(0.01);
    assert_eq!(model.commission(100.0, 50.0), 1.0);
}

#[test]
fn per_trade_flat_commission_ignores_quantity_and_price() {
    let model = CommissionModel::PerTradeFlat(1.5);
    assert_eq!(model.commission(1.0, 10.0), 1.5);
    assert_eq!(model.commission(10_000.0, 500.0), 1.5);
}

#[test]
fn bps_of_notional_commission_scales_with_trade_value() {
    // notional = 100 shares * $50 = $5,000; 2.0 bps of that = $1.00.
    let model = CommissionModel::BpsOfNotional(2.0);
    assert_eq!(model.commission(100.0, 50.0), 1.0);
}
