//
//  TimelineBridge.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/27/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation

struct TimelineCache {
    let ref: UnsafeMutablePointer<timeline>!
    var slides: [TimelineSlideCache]
    let timestamp: timestamp
    let isPlaying: Bool
    
    init(_ ref: UnsafeMutablePointer<timeline>) {
        self.ref = ref;
        
        let raw = ref.pointee;
        let cache = raw.executor.pointee;
        
        self.isPlaying = raw.is_playing != 0
        self.timestamp = raw.seekstamp
        self.slides = []
        for i in 2 ..< cache.slide_count {
            slides.append(TimelineSlideCache(index: i - 1,
                                             title: cache.slides[i].title != nil ? String(cString: cache.slides[i].title) : "",
                                             time: cache.slides[i].seconds,
                                             invalidated: cache.slides[i].trailing_valid == 0))
        }
    }
    
    init(debug timestamp: timestamp, slides: [(title: String, time: TimeInterval)]) {
        self.ref = nil;
        self.timestamp = timestamp;
        self.isPlaying = false
        self.slides = slides.enumerated().map({
            TimelineSlideCache(index: $0, title: $1.title, time: $1.time, invalidated: false)
        })
    }
}

struct TimelineSlideCache: Identifiable {
    let index: Int
    let title: String
    let time: TimeInterval
    let invalidated: Bool
    
    var trueTitle: String {
        "Slide " + String(index)
    }
    
    var id: String {
        trueTitle
    }
}
