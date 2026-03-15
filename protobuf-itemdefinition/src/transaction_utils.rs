use crate::{ItemDefinitionRef, TransactionConsumedItemsComponent, TransactionItem, TransactionRequiredItemsComponent, TransactionRequiredNotHavingItemsComponent, TransactionReward, TransactionRewardedItemsComponent, TransactionRewardedItemsRandomComponent};
use std::sync::Arc;

pub fn convert_transaction_required_items(value: Option<Arc<TransactionRequiredItemsComponent>>) -> Option<Vec<TransactionItem>> {
    value.map(|value| value.required.clone())
}

pub fn convert_transaction_required_not_items(value: Option<Arc<TransactionRequiredNotHavingItemsComponent>>) -> Option<Vec<TransactionItem>> {
    value.map(|value| value.required_not_having.clone())
}

pub fn convert_transaction_consumed(value: Option<Arc<TransactionConsumedItemsComponent>>) -> Option<Vec<TransactionItem>> {
    value.map(|value| value.consumed.clone())
}

pub fn convert_transaction_rewarded(value: Option<Arc<TransactionRewardedItemsComponent>>) -> Option<Vec<TransactionReward>> {
    value.map(|value| value.rewarded.clone())
}

pub fn convert_transaction_rewarded_random(value: Option<Arc<TransactionRewardedItemsRandomComponent>>) -> Option<Vec<ItemDefinitionRef>> {
    value.map(|value| value.reward_pools.clone())
}
