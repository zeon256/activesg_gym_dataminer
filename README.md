# ActiveSG Gym Data Miner
> This repo host the source code that query and parse the slot data for all the ActiveSG Gyms

## Usage
```
Usage: activesg_gym_datamine.exe -u <username> -p <password> [-s]

ActiveSG Slot Dataminer

Options:
  -u, --username    username
  -p, --password    users password
  -s, --is-soa      output data in struct of array
  --help            display usage information
```

## Struct of Array output
You can supply the `-s` flag to output SoA format. The format is something like this.

```json
{
  "gym": "AMK_CC",
  "datetime": "2022-01-11T05:57:33.621402800",
  "time": [
    "2022-01-10T23:00:00Z",
    "2022-01-11T01:00:00Z",
    "2022-01-11T03:00:00Z",
    "2022-01-11T05:00:00Z",
    "2022-01-11T07:00:00Z",
    "2022-01-11T09:00:00Z",
    "2022-01-11T11:00:00Z",
    "2022-01-11T13:00:00Z"
  ],
  "slots_avail": [
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    1
  ]
}
```

## Compile
```
cargo build --release
```