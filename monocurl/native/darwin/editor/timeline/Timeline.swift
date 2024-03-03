//
//  Timeline.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import SwiftUI

//take in slide data
//generate a slide for each (top row)
//the actual mobjects should probably be individually stored
//not sure which makes more sense
//i say we go with mobjects, so that we can still show it on screen if it's past the screen
//maybe we have backend determine the position at each time, and then frontend filters that


//basically just break into discrete rectangles to determine level at any given point
fileprivate let minSecondScale: CGFloat = 0.25;
fileprivate let secondWidth: CGFloat = 40
fileprivate let maxSecondScale: CGFloat = 4;
fileprivate let secondHeight: CGFloat = 20
fileprivate let slideWidth: CGFloat = 80;
fileprivate let slideSeparator: CGFloat = 5;

struct Timeline: View {
    @State private var cache: TimelineCache;
    @EnvironmentObject var environment: MonocurlEnvironment
    
    /* Magnification */
    @State private var startScale: CGFloat = 1;
    @State private var currScale: CGFloat = 1;
 
    init(_ ref: UnsafeMutablePointer<timeline>) {
        self.cache = TimelineCache(ref);
    }
    
    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .top) {
                if !environment.inPresentationMode {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: slideSeparator) {
                            ForEach(self.cache.slides) { slide in
                                TimelineSlide(slide: slide, scale: currScale)
                            }
                            
                            Spacer()
                                .frame(width: proxy.size.width / 2)
                        }
                        .overlay(
                            //cursor
                            Rectangle()
                                .size(width: 2, height: proxy.size.height)
                                .fill(.white)
                                .offset(x: self.timestampToOffset(self.cache.timestamp, scale: currScale))
                        )
                        .contentShape(Rectangle())
                        .gesture(MagnificationGesture()
                            .onChanged { scale in
                                currScale = max(min(startScale * scale, maxSecondScale), minSecondScale);
                            }
                            .onEnded { scale in
                                startScale = currScale
                            }
                        )
                        .gesture(DragGesture(minimumDistance: 0, coordinateSpace: .local)
                            .onEnded { point in
                                let timestamp = self.offsetToTimestamp(point.location.x, scale: currScale);
                                timeline_seek_to(self.cache.ref, timestamp, 1)
                                NSApplication.removeFirstResponder()
                            }
                        )
                        .padding()
                        
                    }
                    
                }
                
                //title
                Toolbar(cache: $cache)
            }
        }
        .onAppear {
            timelineStore = self.$cache
            self.cache = TimelineCache(self.cache.ref)
            timeline_seek_to(self.cache.ref, timestamp(slide: 1, offset: 0), 1)
        }
        .onDisappear {
            timelineStore = nil;
        }
        .frame(minWidth: 500,
               idealWidth: 700,
               maxWidth: .infinity,
               minHeight: environment.inPresentationMode ? 0 : 250,
               idealHeight: environment.inPresentationMode ? 20 : 300,
               maxHeight: environment.inPresentationMode ? 20 : .infinity)

        
    }
    
    func timestampToOffset(_ timestamp: timestamp, scale: CGFloat) -> CGFloat {
        var offset: CGFloat = 0;
        var i = 0;
        while i < self.cache.slides.count && self.cache.slides[i].index < timestamp.slide {
            offset += slideWidth + scale * secondWidth * self.cache.slides[i].time
            offset += slideSeparator
            i += 1
        }
 
        if i == self.cache.slides.count {
            offset -= slideSeparator
        }
        else {
            offset += slideWidth + timestamp.offset * secondWidth * scale;
        }
        
        return offset;
    }
    
    func offsetToTimestamp(_ offset: CGFloat, scale: CGFloat) -> timestamp {
        var comp: CGFloat = 0;
        
        for (i, s) in self.cache.slides.enumerated() {
            let start = comp;
            let end = start + slideWidth + scale * secondWidth * s.time + slideSeparator;
            
            if (offset > start && (offset < end || i == self.cache.slides.count - 1)) {
                return timestamp(slide: s.index, offset: max(0, offset - start - slideWidth) / (scale * secondWidth))
            }
            
            comp = end;
        }
        
        let last = self.cache.slides.last!
        
        return timestamp(slide: last.index, offset: max(0, offset - comp - slideWidth) / (scale * secondWidth));
    }
}

struct TimelineSlide: View {
    let slide: TimelineSlideCache
    let scale: CGFloat
    
    /* in case this becomes animated */
    @State private var seconds: TimeInterval = 0;
    
    var segments: some View {
        HStack(spacing: 0) {
            ForEach(Array(0 ..< Int(seconds)), id: \.self) { i in
                Rectangle().frame(width: scale * secondWidth - 1, height: 1)
                Rectangle().frame(width: 1, height: secondHeight)
            }

            Rectangle()
                .frame(width: scale * secondWidth * seconds.truncatingRemainder(dividingBy: 1.0), height: 1)
        }
        .transition(.move(edge: .leading))
        .onAppear {
            seconds = slide.time
        }
        .onChange(of: slide.time) { time in
            seconds = time
        }
    }
    
    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            VStack {
                ZStack(alignment: .top) {
                    RoundedRectangle(cornerRadius: 5)
                        .stroke(slide.invalidated ? .gray : .yellow)
                        .animation(Animation.easeIn(duration: 0.3), value: slide.invalidated)
                        .frame(width: slideWidth, height: 60)
                    
                    Text(String(format: "%.2fs", slide.time))
                        .font(.system(size: 10))
                        .padding(5)
                        .frame(maxWidth: slideWidth)
                }
                
                Text(slide.trueTitle)
                    .font(.body)
                    .frame(width: slideWidth)
            }
            
            self.segments
                .frame(height: 60)
        }
        .frame(maxHeight: .infinity)
    }
}
