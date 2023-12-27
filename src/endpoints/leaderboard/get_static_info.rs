/*
this endpoint will return static data of leaderboard and position of user address
Steps to get data over different time intervals :
1) iterate over one week timestamps and add total points and get top 3 and get user position
2) iterate over one month timestamps and add total points and get top 3 and get user position
3) iterate over all timestamps and add total points and get top 3 and get user position
*/

use crate::{models::AppState, utils::get_error};
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};

use futures::TryStreamExt;
use mongodb::bson::{doc, Document};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use axum::http::header;
use axum::response::Response;
use chrono::Utc;

#[derive(Debug, Serialize, Deserialize)]
pub struct GetLeaderboardInfoQuery {
    /*
    user address
    */
    addr: String,
    /*
    start of the timestamp range
    -> How many days back you want to start the leaderboard
     */
    start_timestamp: i64,

    /*
    end of the timestamp range
    -> When do you want to end it (ideally the moment the frontend makes the request till that timestamp)
    */
    end_timestamp: i64,
}

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetLeaderboardInfoQuery>,
) -> impl IntoResponse {
    let addr: String = query.addr.to_string();
    let collection = state.db.collection::<Document>("leaderboard_table");
    let start_timestamp = query.start_timestamp;
    let end_timestamp = query.end_timestamp;


    let leaderboard_pipeline = vec![
        doc! {
            "$match": doc! {
                "timestamp": doc! {
                    "$gte": start_timestamp,
                    "$lte": end_timestamp
                }
            }
        },
        doc! {
            "$sort": doc! {
                "experience": -1,
                "timestamp": 1,
                "_id": 1
            }
        },
        doc! {
            "$facet": doc! {
                "best_users": [
                    doc! {
                        "$limit": 3
                    },
                    doc! {
                        "$lookup": doc! {
                            "from": "achieved",
                            "localField": "_id",
                            "foreignField": "addr",
                            "as": "associatedAchievement"
                        }
                    },
                    doc! {
                        "$project": doc! {
                            "_id": 0,
                            "address": "$_id",
                            "xp": "$experience",
                            "achievements": doc! {
                                "$size": "$associatedAchievement"
                            }
                        }
                    }
                ],
                "total_users": [
                    doc! {
                        "$count": "total"
                    }
                ],
                "rank": [
                    doc! {
                        "$addFields": doc! {
                            "tempSortField": 1
                        }
                    },
                    doc! {
                        "$setWindowFields": doc! {
                            "sortBy": doc! {
                                "tempSortField": -1
                            },
                            "output": doc! {
                                "rank": doc! {
                                    "$documentNumber": doc! {}
                                }
                            }
                        }
                    },
                    doc! {
                        "$match": doc! {
                            "_id": addr
                        }
                    },
                    doc! {
                        "$project": doc! {
                            "_id": 0,
                            "rank": "$rank"
                        }
                    },
                    doc! {
                        "$unwind": "$rank"
                    }
                ]
            }
        },
        doc! {
            "$project": doc! {
                "best_users": 1,
                "total_users": doc! {
                    "$arrayElemAt": [
                        "$total_users.total",
                        0
                    ]
                },
                "position": doc! {
                    "$arrayElemAt": [
                        "$rank.rank",
                        0
                    ]
                }
            }
        },
    ];

    return match collection.aggregate(leaderboard_pipeline, None).await {
        Ok(mut cursor) => {
            while let Some(result) = cursor.try_next().await.unwrap() {

                // Set caching response
                let expires = Utc::now() + chrono::Duration::minutes(5);
                let caching_response = Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CACHE_CONTROL, "public, max-age=300")
                    .header(header::EXPIRES, expires.to_rfc2822())
                    .body(Json(result).to_string());

                return caching_response.unwrap().into_response();
            }
            get_error("Error querying ranks".to_string())
        }
        Err(_err) => get_error("Error querying ranks".to_string()),
    };
}
