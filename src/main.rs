//! Reads bus info and answers questions about routes.

use std::collections::HashMap;

use bitflags::bitflags;

use chrono::{offset::Local, Date, DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Weekday};

use clap::clap_app;

use csv::ReaderBuilder;

use failure::bail;

use serde::Deserialize;

/// The address of the trip update.
pub const TRIP_UPDATE_URL: &str =
    "http://transitdata.cityofmadison.com/TripUpdate/TripUpdates.json";

/// The default number of busses to show for a stop.
pub const DEFAULT_N: usize = 10;

#[derive(Debug, Clone, Deserialize)]
struct Trip {
    route_id: String,
    route_short_name: String,
    service_id: String,
    trip_id: String,
    trip_headsign: String,
    direction_id: String,
    direction_name: String,
    block_id: String,
    shape_id: String,
    shape_code: String,
    trip_type: String,
    trip_sort: String,
    wheelchair_accessible: String,
    bikes_allowed: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Stop {
    stop_id: String,
    stop_code: String,
    stop_name: String,
    stop_desc: String,
    stop_lat: String,
    stop_lon: String,
    agency_id: String,
    jurisdiction_id: String,
    location_type: String,
    parent_station: String,
    relative_position: String,
    cardinal_direction: String,
    wheelchair_boarding: String,
    primary_street: String,
    address_range: String,
    cross_location: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StopTimeRaw {
    trip_id: String,
    stop_sequence: String,
    stop_id: String,
    pickup_type: String,
    drop_off_type: String,
    arrival_time: String,
    departure_time: String,
    timepoint: String,
    stop_headsign: String,
    shape_dist_traveled: String,
}

#[derive(Debug, Clone)]
struct StopTime {
    trip_id: String,
    stop_sequence: String,
    stop_id: String,
    pickup_type: String,
    drop_off_type: String,
    arrival_time: NaiveTime,
    departure_time: NaiveTime,
    timepoint: String,
    stop_headsign: String,
    shape_dist_traveled: String,
}

impl StopTime {
    pub fn from_raw(raw: StopTimeRaw) -> Self {
        Self {
            trip_id: raw.trip_id,
            stop_sequence: raw.stop_sequence,
            stop_id: raw.stop_id,
            pickup_type: raw.pickup_type,
            drop_off_type: raw.drop_off_type,
            arrival_time: NaiveTime::parse_from_str(&raw.arrival_time, "%k:%M:%S")
                .unwrap_or_else(|_| NaiveTime::from_hms(0, 0, 0)),
            departure_time: NaiveTime::parse_from_str(&raw.departure_time, "%k:%M:%S")
                .unwrap_or_else(|_| NaiveTime::from_hms(0, 0, 0)),
            timepoint: raw.timepoint,
            stop_headsign: raw.stop_headsign,
            shape_dist_traveled: raw.shape_dist_traveled,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CalendarRaw {
    service_id: String,
    service_name: String,
    monday: String,
    tuesday: String,
    wednesday: String,
    thursday: String,
    friday: String,
    saturday: String,
    sunday: String,
    start_date: String,
    end_date: String,
}

bitflags! {
    #[derive(Deserialize)]
    struct Days: u8 {
        const MONDAY = 1 << 0;
        const TUESDAY = 1 << 1;
        const WEDNESDAY = 1 << 2;
        const THURSDAY = 1 << 3;
        const FRIDAY = 1 << 4;
        const SATURDAY = 1 << 5;
        const SUNDAY = 1 << 6;
    }
}

impl Days {
    pub fn from_weekday(wd: Weekday) -> Self {
        match wd {
            Weekday::Sun => Days::SUNDAY,
            Weekday::Mon => Days::MONDAY,
            Weekday::Tue => Days::TUESDAY,
            Weekday::Wed => Days::WEDNESDAY,
            Weekday::Thu => Days::THURSDAY,
            Weekday::Fri => Days::FRIDAY,
            Weekday::Sat => Days::SATURDAY,
        }
    }
}

#[derive(Debug, Clone)]
struct Calendar {
    service_id: String,
    service_name: String,
    start_date: NaiveDate,
    end_date: NaiveDate,
    days: Days,
    exceptions: Vec<CalendarDate>,
}

impl Calendar {
    pub fn from_calendar(calendar: CalendarRaw) -> Self {
        let mut days = Days::empty();
        if calendar.sunday == "1" {
            days |= Days::SUNDAY;
        }
        if calendar.monday == "1" {
            days |= Days::MONDAY;
        }
        if calendar.tuesday == "1" {
            days |= Days::TUESDAY;
        }
        if calendar.wednesday == "1" {
            days |= Days::WEDNESDAY;
        }
        if calendar.thursday == "1" {
            days |= Days::THURSDAY;
        }
        if calendar.friday == "1" {
            days |= Days::FRIDAY;
        }
        if calendar.saturday == "1" {
            days |= Days::SATURDAY;
        }

        Self {
            service_id: calendar.service_id,
            service_name: calendar.service_name,
            start_date: NaiveDate::parse_from_str(&calendar.start_date, "%Y%m%d")
                .expect("Error parsing date"),
            end_date: NaiveDate::parse_from_str(&calendar.end_date, "%Y%m%d")
                .expect("Error parsing date"),
            days,
            exceptions: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CalendarDateRaw {
    date: String,
    exception_type: String,
    service_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExceptionType {
    Added,
    Removed,
}

#[derive(Debug, Clone)]
struct CalendarDate {
    date: NaiveDate,
    exception_type: ExceptionType,
    service_id: String,
}

impl CalendarDate {
    pub fn from_raw(raw: CalendarDateRaw) -> Self {
        Self {
            date: NaiveDate::parse_from_str(&raw.date, "%Y%m%d").expect("Error parsing date"),
            exception_type: if raw.exception_type == "1" {
                ExceptionType::Added
            } else {
                ExceptionType::Removed
            },
            service_id: raw.service_id,
        }
    }
}

struct StopBusInfo {
    stop_name: String,
    // (trip_short_name, headsign, departure_time, delay in seconds)
    buses: Vec<(String, String, NaiveTime, Option<f64>)>,
}

struct FilterConfig<'s> {
    /// Stop ID
    stop_id: &'s str,

    /// List buses at or after `after`
    after: DateTime<Local>,

    /// How many buses to list?
    how_many: Option<usize>,

    /// Which route to list? If none, list all.
    route: Option<&'s str>,
}

impl<'s> FilterConfig<'s> {
    pub fn new(stop_id: &'s str) -> FilterConfig<'s> {
        Self {
            stop_id,
            after: Local::now(),
            how_many: None,
            route: None,
        }
    }

    pub fn after(self, after: DateTime<Local>) -> Self {
        Self { after, ..self }
    }

    pub fn how_many(self, how_many: usize) -> Self {
        Self {
            how_many: Some(how_many),
            ..self
        }
    }

    pub fn route(self, route: &'s str) -> Self {
        Self {
            route: Some(route),
            ..self
        }
    }
}

struct Data {
    pub trips: HashMap<String, Trip>,               // by trip_id
    pub stops: HashMap<String, Stop>,               // by stop_id
    pub calendar: HashMap<String, Calendar>,        // by service_id
    pub stop_times: HashMap<String, Vec<StopTime>>, // by stop_id
}

impl Data {
    pub fn read(data_dir: &str) -> Result<Self, failure::Error> {
        let mut calendar: HashMap<String, Calendar> = ReaderBuilder::new()
            .has_headers(true)
            .from_path(format!("{}/calendar.txt", data_dir))?
            .deserialize()
            .map(|r| r.expect("Unable to deserialize"))
            .map(Calendar::from_calendar)
            .map(|calendar| (calendar.service_id.clone(), calendar))
            .collect();

        for exception in ReaderBuilder::new()
            .has_headers(true)
            .from_path(format!("{}/calendar_dates.txt", data_dir))?
            .deserialize()
            .map(|r: Result<CalendarDateRaw, _>| r.expect("Unable to deserialize"))
            .map(CalendarDate::from_raw)
        {
            calendar
                .get_mut(&exception.service_id)
                .expect("No such service.")
                .exceptions
                .push(exception);
        }

        let mut stop_times = HashMap::new();

        for stop_time in ReaderBuilder::new()
            .has_headers(true)
            .from_path(format!("{}/stop_times.txt", data_dir))?
            .deserialize()
            .map(|r| r.expect("Unable to deserialize"))
            .map(StopTime::from_raw)
        {
            stop_times
                .entry(stop_time.stop_id.clone())
                .or_insert(vec![])
                .push(stop_time);
        }

        Ok(Self {
            trips: ReaderBuilder::new()
                .has_headers(true)
                .from_path(format!("{}/trips.txt", data_dir))?
                .deserialize()
                .map(|r| r.expect("Unable to deserialize"))
                .map(|trip: Trip| (trip.trip_id.clone(), trip))
                .collect(),
            stops: ReaderBuilder::new()
                .has_headers(true)
                .from_path(format!("{}/stops.txt", data_dir))?
                .deserialize()
                .map(|r| r.expect("Unable to deserialize"))
                .map(|stop: Stop| (stop.stop_id.clone(), stop))
                .collect(),
            stop_times,
            calendar,
        })
    }

    /// Get buses at the stop matching the given filter and the real-time delay info.
    pub fn stop_sched(
        &self,
        conf: FilterConfig,
        real_time: HashMap<String, HashMap<String, f64>>,
    ) -> Result<StopBusInfo, failure::Error> {
        fn to_local(naive: NaiveDate) -> Date<Local> {
            Local::today()
                .timezone()
                .from_local_date(&naive)
                .single()
                .expect("ambiguous date")
        }

        fn to_local_time(naive: NaiveTime) -> DateTime<Local> {
            Local::today().and_time(naive).expect("invalid date/time")
        }

        if let Some(stop) = self.stops.get(conf.stop_id) {
            let buses = self
                .stop_times
                .get(conf.stop_id)
                .cloned()
                .unwrap_or_else(|| vec![]);

            // Filter buses that don't come today.
            let now = conf.after;
            let today = conf.after.date();
            let day = today.weekday();
            let mut buses: Vec<_> = buses
                .iter()
                .filter_map(|bus| {
                    let trip = self.trips.get(&bus.trip_id).expect("Trip id not found");
                    let service = self
                        .calendar
                        .get(&trip.service_id)
                        .expect("Service id not found");

                    // Filter routes.
                    if let Some(route) = conf.route {
                        if trip.route_short_name != route.to_string() {
                            return None;
                        }
                    }

                    // Check that today is in the range and on the right day of the week and not
                    // during an exception.
                    //
                    // Moreover, filter out buses that already came.
                    if to_local(service.start_date) > today {
                        None
                    } else if to_local(service.end_date) < today {
                        None
                    } else if !service.days.contains(Days::from_weekday(day)) {
                        None
                    } else if service.exceptions.iter().any(|ex| {
                        to_local(ex.date) == today
                            && service.service_id == ex.service_id
                            && ex.exception_type == ExceptionType::Removed
                    }) {
                        None
                    } else if to_local_time(bus.departure_time) < now {
                        None
                    } else {
                        // Check for real-time delays.
                        let delay = real_time
                            .get(conf.stop_id)
                            .iter()
                            .flat_map(|stop| stop.get(&bus.trip_id).cloned())
                            .next();

                        Some((
                            trip.route_short_name.clone(),
                            trip.trip_headsign.clone(),
                            bus.departure_time,
                            delay,
                        ))
                    }
                })
                .collect();

            buses.sort_by_key(|(_, _, time, delay)| {
                time.overflowing_add_signed(chrono::Duration::seconds(delay.unwrap_or(0.0) as i64))
            });

            if let Some(len) = conf.how_many {
                buses.truncate(len);
            }

            Ok(StopBusInfo {
                stop_name: stop.stop_name.clone(),
                buses,
            })
        } else {
            bail!("No such bus stop")
        }
    }

    pub fn search(&self, string: Vec<&str>) -> Vec<(String, String)> {
        let strings: Vec<_> = string.iter().map(|s| s.to_lowercase()).collect();

        let mut stops: Vec<(String, String)> = self
            .stops
            .values()
            .filter_map(|stop| {
                if strings
                    .iter()
                    .all(|string| stop.stop_name.to_lowercase().contains(string))
                {
                    Some((stop.stop_id.clone(), stop.stop_name.clone()))
                } else {
                    None
                }
            })
            .collect();

        stops.sort();

        stops
    }
}

macro_rules! warn_and_skip {
    ($json:ident, $key:literal) => {{
        if $json.has_key($key) {
            $json.remove($key)
        } else {
            println!("Key {} not found in {}", $key, stringify!($json));
            continue;
        }
    }};
}

// Hack your way through the real time data and produce by-stop-by-trip delay info.
//
// {stop_id: {trip_id: delay}}
fn parse_real_time_data(
    mut real_time_json: json::JsonValue,
) -> Result<HashMap<String, HashMap<String, f64>>, failure::Error> {
    assert!(real_time_json.has_key("entity"));

    let mut by_stop_id_by_trip_id: HashMap<String, HashMap<String, f64>> = HashMap::new();

    let mut entity = real_time_json.remove("entity");
    for update in entity.members_mut() {
        let mut trip_update = warn_and_skip!(update, "trip_update");
        let mut trip = warn_and_skip!(trip_update, "trip");
        let mut stop_time_update = warn_and_skip!(trip_update, "stop_time_update");
        let trip_id = warn_and_skip!(trip, "trip_id")
            .as_str()
            .expect("expected str")
            .to_owned();
        let rolling_delay = 0.0;
        for stop_time in stop_time_update.members_mut() {
            let stop_id = warn_and_skip!(stop_time, "stop_id")
                .as_str()
                .expect("expected str")
                .to_owned();
            let mut departure = warn_and_skip!(stop_time, "departure");
            let delay = if departure.has_key("delay") {
                departure.remove("delay").as_f64().expect("expected usize")
            } else {
                rolling_delay
            };

            if delay > 0.0 {
                by_stop_id_by_trip_id
                    .entry(stop_id)
                    .or_default()
                    .insert(trip_id.clone(), delay);
            }
        }
    }
    Ok(by_stop_id_by_trip_id)
}

fn print_delay(delay: chrono::Duration) -> String {
    if delay >= chrono::Duration::minutes(1) {
        let minutes = delay.num_minutes();
        let seconds = delay.num_seconds() - minutes * 60;
        if seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else {
        let seconds = delay.num_seconds();
        format!("{}s", seconds)
    }
}

fn main() -> Result<(), failure::Error> {
    let matches = clap_app! { bus =>
        (about: "Info about scheduled buses.")
        (@subcommand stop =>
            (about: "lists the next scheduled buses at the given stop")
            (@arg STOP: +required "The stop ID")
            (@arg WHEN: +takes_value --after -a {is_time}
             "List stops at or after the given time (local to Madison) today \
             (HH:MM, 24-hour clock).")
            (@arg N: +takes_value --next -n {is_usize}
             "List the next N buses.")
            (@arg ROUTE: +takes_value --route -r {is_usize}
             "List only busses taking route ROUTE.")
        )
        (@subcommand search =>
            (about: "Searches for all bus stops that contain the given string")
            (@arg STR: +required ... "The string(s) to search for")
        )
    }
    .setting(clap::AppSettings::SubcommandRequiredElseHelp)
    .get_matches();

    // Read the static bus schedule data.
    let data_dir = std::env::var("BUS_DATA").unwrap_or("data".into());
    let data = Data::read(&data_dir)?;

    // Do computations and print stuff.
    match matches.subcommand() {
        ("stop", Some(sub_m)) => {
            let stop = sub_m.value_of("STOP").unwrap();

            let mut filter = FilterConfig::new(stop);

            if let Some(after) = sub_m.value_of("WHEN") {
                filter = filter.after(
                    Local::today()
                        .and_time(
                            NaiveTime::parse_from_str(after, "%H:%M")
                                .unwrap_or_else(|_| NaiveTime::from_hms(0, 0, 0)),
                        )
                        .expect("invalid date/time"),
                );
            }

            filter = filter.how_many(
                sub_m
                    .value_of("N")
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(DEFAULT_N),
            );

            if let Some(route) = sub_m.value_of("ROUTE") {
                filter = filter.route(route);
            }

            // Read the real time trip update.
            let real_time_json_raw = reqwest::get(TRIP_UPDATE_URL)?.text();
            let real_time_info = if let Ok(real_time_json_raw) = real_time_json_raw {
                if let Ok(real_time_json) = json::parse(&real_time_json_raw) {
                    if let Ok(real_time_json) = parse_real_time_data(real_time_json) {
                        real_time_json
                    } else {
                        println!("WARNING: Unable to parse real-time data.");
                        Default::default()
                    }
                } else {
                    println!("WARNING: Unable to parse real-time data json.");
                    Default::default()
                }
            } else {
                println!("WARNING: Unable to fetch real-time data json.");
                Default::default()
            };

            let bus_info = data.stop_sched(filter, real_time_info)?;

            println!("{}", bus_info.stop_name);
            for (bus, headsign, time, delay) in bus_info.buses.iter() {
                println!(
                    "{} {:10} {}  {}",
                    time.format("%l:%M %p"),
                    if let Some(delay) = delay {
                        format!(
                            "+ {}",
                            print_delay(chrono::Duration::seconds(*delay as i64))
                        )
                    } else {
                        "".into()
                    },
                    bus,
                    headsign,
                )
            }
            if bus_info.buses.is_empty() {
                println!("[No more buses today]");
            }
        }

        ("search", Some(sub_m)) => {
            let strings = sub_m.values_of("STR").unwrap().collect();
            let stops = data.search(strings);

            for (id, stop) in stops {
                println!("{} {}", id, stop);
            }
        }

        _ => unreachable!(),
    }

    Ok(())
}

fn is_usize(s: String) -> Result<(), String> {
    s.as_str()
        .parse::<usize>()
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

fn is_time(s: String) -> Result<(), String> {
    let naive = NaiveTime::parse_from_str(&s, "%H:%M")
        .map_err(|e| format!("Could not parse time: {}", e))?;

    if Local::today().and_time(naive).is_none() {
        Err("Ambiguous time".into())
    } else {
        Ok(())
    }
}
