import assert from 'node:assert/strict'
import test from 'node:test'
import {
  describeSchedule,
  describeTaskSchedule,
  parseFriendlySchedule,
  parseTimeInput,
  scheduleToCron,
  timeInputValue,
} from './schedule.ts'

test('builds cron from everyday schedule choices', () => {
  assert.equal(scheduleToCron({ kind: 'none', hour: 9, minute: 0, weekday: 0 }), undefined)
  assert.equal(scheduleToCron({ kind: 'hourly', hour: 9, minute: 0, weekday: 0 }), '0 * * * *')
  assert.equal(scheduleToCron({ kind: 'daily', hour: 9, minute: 0, weekday: 0 }), '0 9 * * *')
  assert.equal(scheduleToCron({ kind: 'weekdays', hour: 9, minute: 30, weekday: 1 }), '30 9 * * 1-5')
  assert.equal(scheduleToCron({ kind: 'weekly', hour: 2, minute: 0, weekday: 0 }), '0 2 * * 0')
})

test('parses stored cron back into everyday schedule choices', () => {
  assert.equal(parseFriendlySchedule(undefined).kind, 'none')
  assert.equal(parseFriendlySchedule('0 * * * *').kind, 'hourly')
  assert.deepEqual(parseFriendlySchedule('0 9 * * *'), {
    kind: 'daily',
    hour: 9,
    minute: 0,
    weekday: 0,
  })
  assert.deepEqual(parseFriendlySchedule('0 0 9 * * 1-5'), {
    kind: 'weekdays',
    hour: 9,
    minute: 0,
    weekday: 1,
  })
  assert.deepEqual(parseFriendlySchedule('0 2 * * 0'), {
    kind: 'weekly',
    hour: 2,
    minute: 0,
    weekday: 0,
  })
})

test('describes schedules without cron syntax', () => {
  const daily = describeSchedule({ kind: 'daily', hour: 9, minute: 0, weekday: 0 })
  assert.match(daily, /every day/i)
  assert.doesNotMatch(daily, /\*/)
  assert.equal(describeTaskSchedule('0 * * * *'), 'Every hour')
  assert.match(describeTaskSchedule('0 9 * * *', true), /auto-run is off/i)
})

test('roundtrips time inputs', () => {
  assert.equal(timeInputValue(9, 5), '09:05')
  assert.deepEqual(parseTimeInput('18:45'), { hour: 18, minute: 45 })
})
