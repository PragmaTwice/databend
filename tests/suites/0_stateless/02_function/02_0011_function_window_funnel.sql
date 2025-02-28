drop table if exists funnel_test;

create table funnel_test (timestamp UInt32, d Date, dt DateTime(0), event UInt32) Engine = Fuse;
insert into funnel_test values
(0, '2021-01-01', '2021-01-01 00:00:00', 1000),
(1, '2021-01-02', '2021-01-01 00:00:01', 1001),
(2, '2021-01-03', '2021-01-01 00:00:02', 1002),
(3, '2021-01-04', '2021-01-01 00:00:03', 1003),
(4, '2021-01-05', '2021-01-01 00:00:04', 1004),
(5, '2021-01-06', '2021-01-01 00:00:05', 1005),
(6, '2021-01-07', '2021-01-01 00:00:06', 1006),
(7, '2021-01-08', '2021-01-01 00:00:07', 1007),
(8, '2021-01-09', '2021-01-01 00:00:08', 1008);

select 1 = window_funnel(10000)(timestamp, event = 1000) from funnel_test;
select 2 = window_funnel(10000)(timestamp, event = 1000, event = 1001) from funnel_test;
select 3 = window_funnel(10000)(timestamp, event = 1000, event = 1001, event = 1002) from funnel_test;
select 4 = window_funnel(10000)(timestamp, event = 1000, event = 1001, event = 1002, event = 1008) from funnel_test;

select 1 = window_funnel(10000)(d, event = 1000) from funnel_test;
select 2 = window_funnel(10000)(d, event = 1000, event = 1001) from funnel_test;
select 3 = window_funnel(10000)(d, event = 1000, event = 1001, event = 1002) from funnel_test;
select 4 = window_funnel(10000)(d, event = 1000, event = 1001, event = 1002, event = 1008) from funnel_test;

select 1 = window_funnel(10000000000)(dt, event = 1000) from funnel_test;
select 2 = window_funnel(10000000000)(dt, event = 1000, event = 1001) from funnel_test;
select 3 = window_funnel(10000000000)(dt, event = 1000, event = 1001, event = 1002) from funnel_test;
select 4 = window_funnel(10000000000)(dt, event = 1000, event = 1001, event = 1002, event = 1008) from funnel_test;

select 1 = window_funnel(1)(timestamp, event = 1000) from funnel_test;
select 3 = window_funnel(2)(timestamp, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;
select 4 = window_funnel(3)(timestamp, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;
select 5 = window_funnel(4)(timestamp, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;

select 1 = window_funnel(1)(d, event = 1000) from funnel_test;
select 3 = window_funnel(2)(d, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;
select 4 = window_funnel(3)(d, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;
select 5 = window_funnel(4)(d, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;

select 1 = window_funnel(1000000)(dt, event = 1000) from funnel_test;
select 3 = window_funnel(2000000)(dt, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;
select 4 = window_funnel(3000000)(dt, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;
select 5 = window_funnel(4000000)(dt, event = 1003, event = 1004, event = 1005, event = 1006, event = 1007) from funnel_test;

drop table funnel_test;
