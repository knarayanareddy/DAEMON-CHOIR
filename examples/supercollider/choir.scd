/*
==========================================================================
                      DAEMON CHOIR - SuperCollider Template
==========================================================================
This script bootstraps an audio synthesis engine in SuperCollider that maps 
normalized eBPF kernel telemetry OSC events from the Conductor daemon to saw 
oscillators, Moog filters, and percussive noise triggers.

To run:
1. Boot the SuperCollider server (Ctrl+B / Cmd+B).
2. Evaluate this entire script block (Ctrl+Enter / Cmd+Enter).
3. Start the DAEMON CHOIR conductor in simulation or real kernel mode.
==========================================================================
*/

s.boot;

(
// 1. Define synthesis voices matching the Conductor telemetry addresses

// Voice 0 & 2: Telemetry saw synthesizer with a Moog filter
SynthDef(\telemetryVoice, { |out=0, freq=440, amp=0.0, filterFreq=1200, res=0.5, gate=1|
    var env = EnvGen.kr(Env.asr(0.01, 1, 0.1), gate, doneAction: 2);
    // Smooth input parameters using Lag to eliminate network click artifacts
    var osc = Saw.ar(Lag.kr(freq, 0.05)) * Lag.kr(amp, 0.05);
    var filter = MoogFF.ar(osc, Lag.kr(filterFreq, 0.05), res);
    Out.ar(out, filter * env ! 2);
}).add;

// Voice 1: Deep FM bass mapped to memory pressure reclaim counts
SynthDef(\fmbass, { |out=0, freq=110, amp=0.0, modIndex=5.0, gate=1|
    var env = EnvGen.kr(Env.asr(0.01, 1, 0.2), gate, doneAction: 2);
    var modulator = SinOsc.ar(freq * 2) * Lag.kr(modIndex, 0.1) * freq;
    var carrier = SinOsc.ar(Lag.kr(freq, 0.05) + modulator) * Lag.kr(amp, 0.05);
    Out.ar(out, carrier * env ! 2);
}).add;

// Voice 3: White noise snare triggered on new process executions
SynthDef(\percussiveTrigger, { |out=0, amp=0.0|
    var env = EnvGen.kr(Env.perc(0.005, 0.25), doneAction: 2);
    var noise = WhiteNoise.ar(amp * 0.35) + SinOsc.ar(70, 0, amp * 0.65);
    var bandpass = BPF.ar(noise, 1000, 0.4);
    Out.ar(out, (bandpass * 1.5) * env ! 2);
}).add;
)

(
// 2. Instantiate persistent synthesizer objects on the server
~cpuVoice = Synth(\telemetryVoice, [\freq, 220, \amp, 0, \filterFreq, 1000, \res, 0.2]);
~memVoice = Synth(\fmbass, [\freq, 55, \amp, 0, \modIndex, 1.0]);
~netVoice = Synth(\telemetryVoice, [\freq, 330, \amp, 0, \filterFreq, 1200, \res, 0.6]);

// 3. Define OSC responders listening on loopback UDP port 57120

// CPU Latency (p95) maps to oscillator frequency
OSCdef(\cpuFreq, { |msg|
    var val = msg[1];
    // msg format: ['/choir/voice/0/frequency', floatValue]
    "CPU Freq Mapped: % Hz".format(val).postln;
    ~cpuVoice.set(\freq, val);
}, '/choir/voice/0/frequency');

// CPU Context Switch Rate maps to overall volume
OSCdef(\cpuAmp, { |msg|
    var val = msg[1];
    ~cpuVoice.set(\amp, val * 0.25); // Scale maximum volume to preserve hearing
}, '/choir/voice/0/amplitude');

// Memory Reclaim Pressure maps to FM bass frequency and modulation index
OSCdef(\memFreq, { |msg|
    var val = msg[1];
    "Memory Pressure Freq: % Hz".format(val).postln;
    ~memVoice.set(\freq, val);
    ~memVoice.set(\amp, if(val > 60, { 0.3 }, { 0.0 })); // Throb on pressure
    ~memVoice.set(\modIndex, (val - 55) * 0.1);
}, '/choir/voice/1/frequency');

// Network Tx Rate maps to filter resonance (higher bandwidth = squelchier filter)
OSCdef(\netRes, { |msg|
    var val = msg[1];
    ~netVoice.set(\res, val);
}, '/choir/voice/2/filter_resonance');

// Network Rx Rate maps to net voice frequency and volume
OSCdef(\netFreq, { |msg|
    var val = msg[1];
    ~netVoice.set(\freq, val);
    ~netVoice.set(\amp, if(val > 440, { 0.2 }, { 0.0 }));
}, '/choir/voice/2/frequency');

// Process Executions trigger an immediate snare drum transient
OSCdef(\execTrigger, { |msg|
    var val = msg[1];
    if(val > 0.1, {
        "Process Executed! Transient Triggered.".postln;
        Synth(\percussiveTrigger, [\amp, val * 0.8]);
    });
}, '/choir/voice/3/trigger');

"==========================================================".postln;
"DAEMON CHOIR OSC Synthesis responders loaded successfully.".postln;
"Ensure Conductor is running and sending data to 127.0.0.1:57120".postln;
"==========================================================".postln;
)
