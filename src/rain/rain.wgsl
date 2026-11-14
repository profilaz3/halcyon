// Halcyon wallpaper weather. Built after glass.wgsl, whose lens outline and
// lighting it uses. Analytic drops, no ray marching or GPU readback.
//
// Rain is two passes plus composition:
// - a reduced-resolution water field, premultiplied by coverage: shaped lens
//   coordinates (xy), radius (z, negative for water that does not wipe the
//   glass, such as beads and fresh impacts) and coverage (w). Small lens
//   quads for the moving and fresh drops, blended in order. The static beads
//   are cached once in the same encoding;
// - a condensation field at a quarter of the water field's size that keeps
//   its previous frame: moving drops wipe a path about half their width, and
//   fog creeps back in from its edges over 4 to 30 seconds. It also carries
//   the mist-blurred wallpaper the fog shows, so composition reads both in
//   one sample;
// - full-resolution composition: the beads the condensation allows under
//   this frame's drops, sharp wallpaper refracted through each drop, fogged
//   glass between them, rim shading and highlights.
struct Params {
    frame: vec4<f32>, // aspect, time, intensity, size
    motion: vec4<f32>, // speed, wind, mist, refraction
    style: vec4<f32>, // mode, opacity, trails, fade
    crop: vec4<f32>, // visible UV origin/extent; scroll clipping keeps the image fixed
    atmosphere: vec4<f32>, // rain: elapsed seconds, field texel, detail; others: opacity, height, softness
    tint: vec4<f32>, // rgb; w: one output pixel in UV height
    behind: vec4<f32>, // where the picture behind the glass lies, in UV; none when empty
};
@group(0) @binding(0) var<uniform> p: Params;
@group(1) @binding(0) var background: texture_2d<f32>;
@group(1) @binding(1) var linear_sampler: sampler;
@group(1) @binding(2) var dynamic_field: texture_2d<f32>;
@group(1) @binding(3) var condensation: texture_2d<f32>;
@group(1) @binding(4) var beads: texture_2d<f32>;
// A picture between the wallpaper and the glass (the desktop clock),
// premultiplied.
@group(1) @binding(5) var behind: texture_2d<f32>;
struct Vertex { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
fn triangle(index:u32) -> Vertex {
    let positions=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
    let xy=positions[index];
    return Vertex(vec4(xy,0.,1.), vec2(xy.x*0.5+0.5,0.5-xy.y*0.5));
}
@vertex fn fullscreen(@builtin(vertex_index) index:u32) -> Vertex { return triangle(index); }
@vertex fn composite_vertex(@builtin(vertex_index) index:u32) -> Vertex {
    var v=triangle(index);
    v.uv=p.crop.xy+v.uv*p.crop.zw;
    return v;
}
// Hash without Sine, David Hoskins (MIT): https://www.shadertoy.com/view/4djSRW
fn hash(n:vec2<f32>) -> f32 {
    let h=fract(vec3(n.x,n.y,n.x)*vec3(0.1031,0.1030,0.0973));
    let q=h+dot(h,h.yzx+33.33);
    return fract((q.x+q.y)*q.z);
}
fn hash3(n:vec2<f32>) -> vec3<f32> {
    var h=fract(vec3(n.x,n.y,n.x)*vec3(0.1031,0.1030,0.0973));
    h+=dot(h,h.yxz+33.33);
    return fract((h.xxy+h.yzz)*h.zyx);
}


// One encoded lens. `wipes` is false for water that stays on the glass.
fn encode(q:vec2<f32>, cover:f32, radius:f32, wipes:bool) -> vec4<f32> {
    return vec4(q*cover, select(-radius, radius, wipes)*cover, cover);
}
fn over(top:vec4<f32>, bottom:vec4<f32>) -> vec4<f32> {
    return top+bottom*(1.-clamp(top.w,0.,1.));
}
fn lens_cover(q:vec2<f32>, radius:f32) -> f32 {
    let edge=min(0.7, 1.5*p.atmosphere.y/max(radius,1e-5));
    return 1.-smoothstep(1.-edge,1.+edge,dot(q,q));
}
fn drop_scale() -> f32 { return 0.55+p.frame.w*1.1; }
fn pace() -> f32 { return 0.3+p.motion.x*1.4; }

// Procedural time wraps every hour (rain.rs). Every cycle length and
// oscillation divides the hour exactly, so the field picks up exactly where it was
// across the wrap instead of jumping to a new pattern.
const PERIOD:f32=3600.;
fn repeats(length:f32) -> f32 { return max(1.,round(PERIOD/max(length,1e-3))); }
// Which of `count` cycles in the hour `phase` is in. GPUs divide through a
// reciprocal, so count/count may come out a hair under 1; half a cycle of
// margin keeps whole numbers whole.
fn cycle_index(phase:f32, count:f32) -> f32 {
    let cycle=floor(phase);
    return cycle-count*floor((cycle+0.5)/count);
}
// This moment's phase, in radians, of an oscillation near `rate` radians
// per second, rounded to complete whole turns in the hour. Taken from the
// fraction of the hour, so it stays precise late in the hour.
fn hourly(rate:f32) -> f32 {
    let turns=max(1.,round(rate*PERIOD/6.2831853));
    return fract(p.frame.y/PERIOD*turns)*6.2831853;
}

// Condensation beads, plus smaller beads that sliding drops leave behind in
// their cleared paths (negative radius). Drawn once into a cached texture, so
// the neighbour search costs nothing per frame. Small water on glass is
// round: only the largest beads get a slightly uneven outline.
@fragment fn static_beads(v:Vertex) -> @location(0) vec4<f32> {
    let xy=v.uv*vec2(p.frame.x,1.);
    let amount=p.frame.z;
    var field=vec4(0.);
    if amount<=0. {return field;}
    let size=0.75+p.frame.w*0.5;
    // Patches of denser and sparser beading, as on real glass.
    let patches=mix(0.35,1.45,noise(xy*4.+vec2(3.1,7.7)));
    // Eco skips the two finest, densest layers.
    for(var layer=select(2,0,p.atmosphere.z>0.);layer<4;layer+=1) {
        let residue=layer==3;
        let tile=array<f32,4>(0.0125,0.0175,0.024,0.02)[layer]*size;
        // Each layer's grid is turned by its own angle, and a bead may sit
        // anywhere in most of its cell, so no rows or columns line up. The
        // two nearest cells on each axis cover every bead that can reach.
        let turn=array<f32,4>(0.43,-0.71,1.13,-0.29)[layer];
        let axes=mat2x2<f32>(cos(turn),sin(turn),-sin(turn),cos(turn));
        let grid=transpose(axes)*xy/tile+vec2(f32(layer)*0.37,f32(layer)*0.61);
        let base=floor(grid-0.5);
        let share=select(0.25+amount*0.65,0.3+amount*0.4,residue)*select(1.,0.7,layer==2)
            *select(patches,1.,residue);
        for(var n=0;n<4;n+=1) {
            let cell=base+vec2(f32(n&1),f32(n>>1));
            let seed=hash3(cell+vec2(17.*f32(layer),5.));
            if seed.x>share {continue;}
            let radius=tile*select(mix(0.08,0.27,seed.z*seed.z*seed.z),mix(0.1,0.2,seed.z),residue);
            let jitter=hash3(cell*1.7+vec2(3.3,9.1*f32(layer))).xy*2.-1.;
            let center=(axes*(cell+0.5+jitter*0.45)-axes*vec2(f32(layer)*0.37,f32(layer)*0.61))*tile;
            let raw=(xy-center)/vec2(radius,radius*mix(0.92,1.04,seed.y));
            if dot(raw,raw)>1.7 {continue;}
            let q=lens_shape(raw,seed,0.12*seed.z*seed.z,0.,vec2(0.));
            let cover=lens_cover(q,radius)*select(0.92,0.8,residue);
            if cover<=0. {continue;}
            field=over(encode(q,cover,radius,!residue),field);
        }
    }
    return field;
}

// Moving and fresh water is drawn as one small quad per drop, so the cost
// follows the wet area rather than the field size. A quad's corners are in
// lens radii; the fragment shapes the outline from them.
struct LensVertex {
    @builtin(position) position:vec4<f32>,
    @location(0) raw:vec2<f32>,
    @location(1) @interpolate(flat) traits:vec3<f32>,
    @location(2) @interpolate(flat) shape:vec4<f32>, // roughness, taper, oval wobble, radius
    @location(3) @interpolate(flat) look:vec3<f32>, // opacity, wipes (1) or stays (0), turning wobble
};
fn hidden() -> LensVertex {
    return LensVertex(vec4(-2.,-2.,0.,1.),vec2(0.),vec3(0.),vec4(0.),vec3(0.));
}
// `center` is in glass units (x scaled by the aspect); `shear` slants the quad
// with the wind so the lens follows its lane; `stretch` elongates it.
fn lens_vertex(index:u32, center:vec2<f32>, radius:f32, stretch:f32, shear:f32,
    traits:vec3<f32>, shape:vec4<f32>, look:vec3<f32>) -> LensVertex {
    let corners=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    // Room for the wobbling outline, a teardrop's wider foot and the rim.
    let edge=min(0.7,1.5*p.atmosphere.y/max(radius,1e-5));
    let raw=corners[index]*(vec2(1.9,1.45)+edge);
    let dy=raw.y*radius*stretch;
    let xy=(center+vec2(raw.x*radius+shear*dy,dy))/vec2(p.frame.x,1.);
    return LensVertex(vec4(xy.x*2.-1.,1.-xy.y*2.,0.,1.),raw,traits,shape,look);
}
@fragment fn lens_fragment(v:LensVertex) -> @location(0) vec4<f32> {
    let q=lens_shape(v.raw,v.traits,v.shape.x,v.shape.y,vec2(v.shape.z,v.look.z));
    let cover=lens_cover(q,v.shape.w)*v.look.x;
    if cover<=0. {discard;}
    return encode(q,cover,v.shape.w,v.look.y>0.5);
}
// This moment's outline oscillation: a slow breathing for resting water, a
// quicker wobble for a drop that slides, stronger while it surges.
fn wobble(traits:vec3<f32>, sliding:f32, velocity:f32) -> vec2<f32> {
    let rate=mix(1.7,mix(7.,11.,traits.x),sliding);
    let phase=hourly(rate)+traits.z*6.2831853;
    let turn=hourly(rate*0.53)+traits.y*4.;
    let amount=mix(0.012,0.025+0.045*min(velocity,1.5),sliding);
    return vec2(amount*sin(phase),(0.08+0.14*sliding)*sin(turn));
}

// Runoff: in each lane a drop gathers water at rest, then slides off the
// glass in surges and pauses of uneven length along a winding path. It
// stretches while it moves, wobbles, sheds water into a trail of tiny beads
// and shrinks as it goes. Wiping drops clear the fog.
struct Run {
    shown:bool,
    x0:f32, // the path's x at the top of the glass, before the wind
    start:f32, // where the drop formed
    descent:f32,
    size:f32, // full radius before shedding
    radius:f32,
    velocity:f32, // relative to the lane's speed
    sliding:f32,
    elapsed:f32,
    hold:f32,
    period:f32,
    traits:vec3<f32>,
    salt:f32,
    span:f32,
    sway:f32,
    lean:f32,
    shear:f32,
};
// Sideways position along a path: mostly straight runs of uneven length,
// with a jog where the drop met a bead or a dry patch, each eased over about
// a drop's height, and a slow lean.
// The stretch of a path `along` (in spans) lies in; stretches begin at
// uneven points.
fn path_piece(along:f32, salt:f32) -> f32 {
    var piece=floor(along);
    if along<piece+0.7*(hash(vec2(piece,salt))-0.5) {piece-=1.;}
    else if along>=piece+1.+0.7*(hash(vec2(piece+1.,salt))-0.5) {piece+=1.;}
    return piece;
}
fn wander(descent:f32, span:f32, salt:f32, sway:f32, lean:f32) -> f32 {
    let along=descent/span;
    let piece=path_piece(along,salt);
    let began=piece+0.7*(hash(vec2(piece,salt))-0.5);
    let ease=smoothstep(0.,0.022/span,along-began);
    return sway*(mix(hash(vec2(piece-1.,salt+0.5)),hash(vec2(piece,salt+0.5)),ease)*2.-1.)
        +lean*descent;
}
// Lane `slot_index`'s drop, `back` cycles ago (0 or 1).
fn run(slot_index:u32, width:f32, scale:f32, seed:f32, slot:f32, back:f32) -> Run {
    var r:Run;
    r.shown=false;
    // Lanes run along x - shear*y, slanted by the wind.
    r.shear=p.motion.y*0.3;
    let lane=floor(min(0.,-r.shear)/width)-1.+f32(slot_index);
    if lane*width>p.frame.x+max(0.,-r.shear)+width {return r;}
    let stream=hash3(vec2(lane,seed));
    let speed=mix(0.16,0.42,stream.x)*pace();
    r.hold=mix(0.8,5.5,stream.z)/pace();
    let count=repeats(r.hold+2.8/speed);
    r.period=PERIOD/count;
    let phase=p.frame.y/r.period+stream.y+slot-back;
    let cycle=cycle_index(phase,count);
    r.elapsed=(fract(phase)+back)*r.period;
    r.traits=hash3(vec2(lane*1.7+seed*53.,cycle+slot*31.));
    if r.traits.x>0.12+p.frame.z*0.88 {return r;}
    // It gathers water in a few merges, then breaks free.
    let gathered=clamp(r.elapsed/(0.85*r.hold),0.,1.)*3.;
    let merges=(floor(gathered)+smoothstep(0.7,1.,fract(gathered)))/3.;
    r.size=0.016*scale*drop_scale()*mix(0.65,1.,r.traits.y)*mix(0.45,1.,merges);
    let slip=max(0.,r.elapsed-r.hold);
    // Stick-slip: surges of uneven length and strength between pauses;
    // heavier drops pause less.
    r.salt=lane*3.1+cycle*7.3+seed;
    var left=slip;
    var descent=0.;
    var velocity=0.;
    for(var k=0;k<20;k+=1) {
        if left<=0. {break;}
        let h=hash3(vec2(r.salt+f32(k)*1.37,11.+slot));
        let go=mix(0.35,1.3,h.x)/pace();
        let pause=mix(0.,0.8,h.y*h.y)*(1.-0.6*r.traits.y)/pace();
        let top=speed*mix(1.,2.6,h.z);
        if left<go {
            let u=left/go;
            descent+=top*go*(u*0.5-sin(6.2831853*u)/12.566371);
            let s=sin(3.1415927*u);
            velocity=top*s*s;
            break;
        }
        descent+=top*go*0.5;
        left-=go+pause;
    }
    r.descent=descent;
    r.velocity=velocity/speed;
    r.sliding=smoothstep(0.,0.25,slip);
    r.start=0.04+r.traits.z*0.7;
    // Water left in the trail: a drop that crossed the glass is about 40%
    // smaller than when it set off.
    r.radius=r.size*(1.-0.4*smoothstep(0.,1.,descent));
    // Each drop takes its own path, so the drops of one lane never retrace a
    // column.
    let path=hash3(vec2(cycle*3.7+slot*17.,lane*2.3+seed));
    r.span=mix(0.08,0.24,path.x);
    r.sway=0.006+width*0.1;
    r.lean=(path.y-0.5)*0.05;
    r.x0=(lane+0.5+(path.z-0.5)*0.7)*width;
    r.shown=true;
    return r;
}
fn run_x(r:Run, descent:f32) -> f32 {
    return r.x0+wander(descent,r.span,r.salt,r.sway,r.lean)+r.shear*(r.start+descent);
}
fn runoff(index:u32, slot_index:u32, width:f32, scale:f32, seed:f32, slot:f32) -> LensVertex {
    let r=run(slot_index,width,scale,seed,slot,0.);
    if !r.shown {return hidden();}
    let y=r.start+r.descent;
    if y-r.radius*2.>1. || r.radius<1e-4 {return hidden();}
    let moving=min(r.velocity,1.6);
    let w=wobble(r.traits,r.sliding,r.velocity);
    // A drop still on the glass when its lane's next one forms runs dry.
    let opacity=smoothstep(0.,0.15,r.elapsed)*(1.-smoothstep(r.period-0.5,r.period,r.elapsed));
    return lens_vertex(index,vec2(run_x(r,r.descent),y),r.radius,1.+0.1*r.sliding+0.34*moving,r.shear,
        r.traits,vec4(0.22,0.16*r.sliding+0.1*moving,w.x,r.radius),
        vec3(opacity,select(0.,1.,r.elapsed>r.hold),w.y));
}

// The trail of tiny beads a sliding drop leaves behind it, along the path
// drawn so far. Older beads dry up first. Once the drop has gone, its trail
// fades while the lane's next drop gathers. The path is drawn as
// TRAIL_PIECES quads stacked down the glass, each only as wide as the
// path's sway inside it, so the beads are worked out only near the path.
const TRAIL_PIECES=10u;
struct TrailVertex {
    @builtin(position) position:vec4<f32>,
    @location(0) glass:vec2<f32>,
    @location(1) @interpolate(flat) path:vec4<f32>, // x0, salt, span, sway
    @location(2) @interpolate(flat) run:vec4<f32>, // start, descent so far, drop size, lean
    @location(3) @interpolate(flat) look:vec2<f32>, // opacity, shear
};
fn no_trail() -> TrailVertex {
    return TrailVertex(vec4(-2.,-2.,0.,1.),vec2(0.),vec4(0.),vec4(0.),vec2(0.));
}
fn trail(index:u32, slot_index:u32, width:f32, scale:f32, seed:f32, slot:f32) -> TrailVertex {
    if p.style.z<=0. {return no_trail();}
    var r=run(slot_index,width,scale,seed,slot,0.);
    var opacity=1.;
    if r.elapsed<=r.hold {
        // Still gathering: show the previous drop's trail drying up.
        let gone=r.elapsed;
        r=run(slot_index,width,scale,seed,slot,1.);
        opacity=1.-smoothstep(0.,min(r.hold,1.5+4.*p.style.z),gone);
    }
    if !r.shown || opacity<=0. {return no_trail();}
    let reach=min(r.descent-r.radius*1.2,1.1-r.start);
    if reach<=0. {return no_trail();}
    let corners=array<vec2<f32>,6>(vec2(-1.,0.),vec2(1.,0.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,0.),vec2(1.,1.));
    let c=corners[index%6u];
    // This quad's share of the path, from a little above its start to a
    // little past its end; the quads do not overlap.
    let end=r.size*0.3;
    let piece=f32(index/6u);
    let pieces=f32(TRAIL_PIECES);
    let along=-end+(reach+2.*end)*(piece+c.y)/pieces;
    // Beads wet the glass this far from their centres (lens_cover ends at
    // 1.3 radii), so this far from their row and from the path.
    let bead=1.3*max(r.size*0.3,1.5*p.atmosphere.y);
    let top=max(-end+(reach+2.*end)*piece/pieces-bead,0.);
    let bottom=min(-end+(reach+2.*end)*(piece+1.)/pieces+bead,reach);
    // The sway between the stretches this quad's beads lie in. A stretch
    // eases over from the one before it in its first 0.022 glass heights.
    var low=1.;
    var high=0.;
    let opened=path_piece(top/r.span,r.salt);
    let began=(opened+0.7*(hash(vec2(opened,r.salt))-0.5))*r.span;
    let first=select(opened-1.,opened,top-began>=0.022);
    let last=max(path_piece(bottom/r.span,r.salt),first);
    for(var k=0;k<8;k+=1) {
        let at=first+f32(k);
        if at>last {break;}
        let h=hash(vec2(at,r.salt+0.5));
        low=min(low,h);
        high=max(high,h);
    }
    let sway=r.sway*(select(low,high,c.x>0.)*2.-1.);
    let margin=r.size*0.225+bead*(1.+abs(r.lean+r.shear));
    let glass=vec2(r.x0+sway+r.lean*along+r.shear*(r.start+along)+c.x*margin,r.start+along);
    let xy=glass/vec2(p.frame.x,1.);
    return TrailVertex(vec4(xy.x*2.-1.,1.-xy.y*2.,0.,1.),glass,
        vec4(r.x0,r.salt,r.span,r.sway),vec4(r.start,reach,r.size,r.lean),vec2(opacity,r.shear));
}
@fragment fn trail_fragment(v:TrailVertex) -> @location(0) vec4<f32> {
    let start=v.run.x;
    let reach=v.run.y;
    let size=v.run.z;
    let gap=max(size*0.75,0.003);
    let d=v.glass.y-start;
    let cell=floor(d/gap);
    // How far behind the drop its beads last, in glass heights.
    let keep=0.3+0.9*p.style.z;
    let finest=1.5*p.atmosphere.y;
    var field=vec4(0.);
    for(var j=-1;j<=1;j+=1) {
        let i=cell+f32(j);
        let h=hash3(vec2(i*1.913+v.path.y,5.7));
        if h.x>0.3+0.55*p.style.z {continue;}
        let at=(i+0.15+0.7*h.y)*gap;
        if at<0. || at>reach {continue;}
        // At least a texel and a half of the water field, so the finest
        // beads still show.
        let bead=max(size*mix(0.1,0.3,h.z*h.z),finest);
        // Rows away from this bead skip the path sums.
        if abs(v.glass.y-start-at)>bead*1.3 {continue;}
        let x=v.path.x+wander(at,v.path.z,v.path.y,v.path.w,v.run.w)+v.look.y*(start+at)
            +(h.y-0.5)*size*0.45;
        let raw=(v.glass-vec2(x,start+at))/bead;
        if dot(raw,raw)>1.7 {continue;}
        let fade=1.-smoothstep(keep*0.5,keep,reach-at);
        // Trail beads are a few pixels across: a slightly squashed circle
        // reads as round without the full outline.
        let q=raw*vec2(1.+0.12*(h.x-0.5),1.+0.12*(h.z-0.5));
        let cover=lens_cover(q,bead)*fade*v.look.x*0.9;
        if cover<=0. {continue;}
        field=over(encode(q,cover,bead,false),field);
    }
    if field.w<=0. {discard;}
    return field;
}

// The condensation as of the last frame, for impacts a drop has absorbed.
@group(1) @binding(0) var glass_fog: texture_2d<f32>;
@group(1) @binding(1) var fog_sampler: sampler;

// Impacts: fresh drops land as a small splash, round into a bead within a
// moment, sit, and dry up; they do not wipe. A sliding drop that passes over
// one takes it in.
fn impact(index:u32, cell_index:u32, tile:f32, seed:f32) -> LensVertex {
    let columns=u32(ceil(p.frame.x/tile))+3u;
    if cell_index>=columns*(u32(ceil(1./tile))+3u) {return hidden();}
    let cell=floor(vec2(seed))-1.+vec2(f32(cell_index%columns),f32(cell_index/columns));
    let stream=hash3(cell+vec2(seed*7.,41.));
    let count=repeats(mix(5.,12.,stream.x)/pace());
    let life=PERIOD/count;
    let phase=p.frame.y/life+stream.y;
    let traits=hash3(cell*1.31+vec2(cycle_index(phase,count)*7.1,seed));
    if traits.x>p.frame.z*0.75 {return hidden();}
    let age=fract(phase);
    let growth=smoothstep(0.,0.03,age);
    let fade=1.-smoothstep(0.7,1.,age);
    let radius=tile*mix(0.07,0.18,traits.y*traits.y)*drop_scale()*growth;
    if radius<1e-4 {return hidden();}
    let center=(cell+vec2(0.5)+(traits.yz*2.-1.)*0.25-seed)*tile;
    // Wiped glass fogs over again at a known pace, so glass clearer than
    // this drop's age allows was wiped after it landed.
    if p.style.z>0. {
        let fog=textureSampleLevel(glass_fog,fog_sampler,center/vec2(p.frame.x,1.),0.).a;
        if fog<0.85 && age*life>fog*(4.+26.*p.style.z)*0.7+0.3 {return hidden();}
    }
    let w=wobble(traits,0.,0.);
    return lens_vertex(index,center,radius,1.,0.,traits,
        vec4(1.-smoothstep(0.,0.06,age)*0.8,0.,w.x,radius),vec3(fade,0.,w.y));
}

// Procedural rain's instances, in drawing order: two impact grids, then three
// sets of runoff lanes. gpu.rs (`procedural_draws`) draws only the slots that
// can reach the glass; any that still fall outside are hidden. Eco skips the
// finer impacts and runoff. The lanes' trails (`trail_vertex`) are drawn
// between the impacts and the runoff.
const CELLS:u32=8192u;
const LANES:u32=512u;
@vertex fn procedural_vertex(@builtin(vertex_index) index:u32, @builtin(instance_index) instance:u32) -> LensVertex {
    var i=instance;
    if i<CELLS {return impact(index,i,0.05,0.);}
    i-=CELLS;
    let detail=p.atmosphere.z>0.;
    if i<CELLS {
        if !detail {return hidden();}
        return impact(index,i,0.034,3.7);
    }
    i-=CELLS;
    if i<LANES {
        if !detail {return hidden();}
        return runoff(index,i,0.034,0.55,19.,0.);
    }
    i-=LANES;
    if i<LANES {return runoff(index,i,0.07,1.,7.,0.);}
    return runoff(index,i-LANES,0.07,1.,7.,0.5);
}
@vertex fn trail_vertex(@builtin(vertex_index) index:u32, @builtin(instance_index) instance:u32) -> TrailVertex {
    var i=instance;
    if i<LANES {
        if p.atmosphere.z<=0. {return no_trail();}
        return trail(index,i,0.034,0.55,19.,0.);
    }
    i-=LANES;
    if i<LANES {return trail(index,i,0.07,1.,7.,0.);}
    return trail(index,i-LANES,0.07,1.,7.,0.5);
}

// Simulated drops from the CPU: x, y, radius, stretch; opacity, taper
// (negative for beads that stay) and a seed for the outline.
@vertex fn drop_vertex(@builtin(vertex_index) index:u32,@location(0) drop:vec4<f32>,@location(1) look:vec3<f32>) -> LensVertex {
    let traits=hash3(vec2(look.z*91.7,look.z*13.3));
    let sliding=select(0.,1.,look.y>=0.);
    let w=wobble(traits,sliding,(drop.w-1.)*2.);
    let rough=0.08+0.18*smoothstep(0.004,0.012,drop.z);
    return lens_vertex(index,drop.xy,drop.z,drop.w,0.,traits,
        vec4(rough,max(look.y,0.),w.x,drop.z),vec3(look.x,sliding,w.y));
}


// Condensation feedback: rgb is the blurred wallpaper, a the fog (1 fogged,
// 0 wiped clear). `water` is the water field.
@group(1) @binding(0) var previous: texture_2d<f32>;
@group(1) @binding(1) var feedback_sampler: sampler;
@group(1) @binding(2) var water: texture_2d<f32>;
@group(1) @binding(3) var wallpaper: texture_2d<f32>;

@fragment fn condense(v:Vertex) -> @location(0) vec4<f32> {
    // Fogged glass shows the wallpaper blurred by the mist.
    let blur=textureSampleLevel(wallpaper,feedback_sampler,v.uv,2.5+2.*p.motion.z).rgb;
    let last=textureSampleLevel(previous,feedback_sampler,v.uv,0.).a;
    if p.style.z<=0. {return vec4(blur,1.);}
    let size=vec2<f32>(textureDimensions(previous));
    let step=0.25/size;
    // A sliding drop wipes a path narrower than itself: only the middle of
    // the lens, about half its width, touches the glass hard enough.
    var wipe=0.;
    for(var i=0;i<4;i+=1) {
        let o=vec2(select(-1.,1.,(i&1)==1),select(-1.,1.,i>=2))*step;
        let f=textureSampleLevel(water,feedback_sampler,v.uv+o,0.);
        if f.z>0. {
            let q=f.xy/max(f.w,1e-4);
            wipe=max(wipe,clamp(f.w,0.,1.)*(1.-smoothstep(0.3,0.6,abs(q.x))));
        }
    }
    // Fog creeps back in from a path's edges, unevenly: the neighbours'
    // fog diffuses in and speeds the regrowth, and each texel has its own
    // pace, so an old path narrows, tapers and breaks into clear spots.
    var near=0.;
    for(var i=0;i<4;i+=1) {
        let o=select(vec2(select(-1.,1.,i==1),0.),vec2(0.,select(-1.,1.,i==3)),i>=2)/size;
        near+=textureSampleLevel(previous,feedback_sampler,v.uv+o,0.).a*0.25;
    }
    let dt=p.atmosphere.x;
    let spread=mix(last,near,min(1.,dt*0.8));
    // Trails decide how long a cleared path takes to fog over.
    let settle=4.+26.*p.style.z;
    let regrow=mix(0.5,1.5,hash(floor(v.uv*size)))*(0.4+1.6*near);
    let fogged=min(1.,spread+dt*regrow/settle);
    return vec4(blur,min(fogged,1.-smoothstep(0.12,0.55,wipe)));
}

fn decode(f:vec4<f32>) -> Water {
    let weight=max(f.w,1e-4);
    var w=lens_light(f.xy/weight,clamp(f.w,0.,1.),p.motion.w);
    w.bend*=abs(f.z)/weight;
    return w;
}
fn luminance(c:vec3<f32>) -> f32 { return dot(c,vec3(0.2126,0.7152,0.0722)); }
// The picture behind the glass at `uv`, premultiplied, or nothing.
fn behind_at(uv:vec2<f32>, lod:f32) -> vec4<f32> {
    if p.behind.z<=0. {return vec4(0.);}
    let local=(uv-p.behind.xy)/p.behind.zw;
    if any(local<vec2(0.)) || any(local>vec2(1.)) {return vec4(0.);}
    return textureSampleLevel(behind,linear_sampler,local,lod);
}
// What lies behind the glass: the wallpaper, and the picture in front of it.
fn scene_at(uv:vec2<f32>, lod:f32) -> vec3<f32> {
    let wall=textureSampleLevel(background,linear_sampler,uv,lod).rgb;
    let front=behind_at(uv,lod);
    return front.rgb+wall*(1.-front.a);
}

// Each effect composes in its own entry point, so rain's registers are not
// sized for the snow loop and the reverse.
fn tinted(uv:vec2<f32>, amount:f32) -> vec4<f32> {
    let clean=scene_at(uv,0.);
    return vec4(mix(clean,p.tint.rgb,clamp(amount,0.,1.)*p.style.w),1.);
}
// Snow, fog and clouds drift without repeating, so near the end of the
// hour their next hour fades in, and the wrap shows the same sky.
const BLEND:f32=8.;
fn hourly_blend() -> f32 { return smoothstep(PERIOD-BLEND,PERIOD,p.frame.y); }
@fragment fn compose_snow(v:Vertex)->@location(0) vec4<f32> {
    var amount=snow(v.uv,p.frame.y);
    let blend=hourly_blend();
    if blend>0. {amount=mix(amount,snow(v.uv,p.frame.y-PERIOD),blend);}
    return tinted(v.uv,amount);
}
@fragment fn compose_atmosphere(v:Vertex)->@location(0) vec4<f32> {
    return tinted(v.uv,textureSampleLevel(dynamic_field,linear_sampler,v.uv,0.).r);
}
@fragment fn composite(v:Vertex)->@location(0) vec4<f32> {
    let glass=textureSampleLevel(condensation,linear_sampler,v.uv,0.);
    let fog=clamp(glass.a,0.,1.);
    // Fog blurs the picture behind the glass too, though less than the
    // distant wallpaper, so it stays readable.
    let misted=behind_at(v.uv,1.+2.*p.motion.z);
    let blur=misted.rgb+glass.rgb*(1.-misted.a);
    var live=textureSampleLevel(dynamic_field,linear_sampler,v.uv,0.);
    // The cached beads lie under this frame's drops. Condensation beads need
    // fogged glass: they vanish where a drop wiped and return as it fogs
    // over. Left-behind beads show only in the cleared path.
    let bead=textureSampleLevel(beads,linear_sampler,v.uv,0.);
    if bead.w>0.001 {
        let beaded=smoothstep(0.45,0.95,fog);
        let left=(1.-smoothstep(0.35,0.85,fog))*step(0.001,p.style.z);
        let under=vec4(bead.xy,abs(bead.z),bead.w)*select(left,beaded,bead.z>=0.);
        live=vec4(live.xy,abs(live.z),live.w)+under*(1.-clamp(live.w,0.,1.));
    }
    // Most of the glass holds no water; skip the lens work there.
    var water=Water(vec2(0.),0.,0.,0.);
    var lod=0.;
    if live.w>0.002 {
        water=decode(live);
        // Read the mip that matches the lens's compression, strongest at the
        // rim, so the view through a moving drop neither sparkles nor
        // thrashes the cache.
        let q=live.xy/max(live.w,1e-4);
        let r2=min(dot(q,q),1.);
        lod=log2(max(1.,(0.55+0.7*p.motion.w)*(1.+2.4*r2*r2)))*water.clear;
    }
    // A drop is a lens: the sharp wallpaper, flipped and slightly magnified.
    // Dry glass reads the same texel unbent, so each pixel fetches the
    // wallpaper once.
    let bend=water.bend*vec2(1./p.frame.x,1.);
    let uv=clamp(v.uv+bend,vec2(0.001),vec2(0.999));
    let seen=scene_at(uv,lod);
    // The blurred wallpaper's brightness lights the glass.
    let light=luminance(blur);
    // Fogged glass scatters: blurred, lighter and less saturated. Water
    // clears it.
    let frosted=mix(blur,vec3(light),0.25)*0.8+vec3(0.07+0.16*light);
    var rgb=mix(seen,frosted,fog*min(1.,p.motion.z*2.)*0.8*(1.-water.clear));
    // The wallpaper's own brightness lights the rim and highlight.
    let shine=min(water.shine*(0.55+0.9*light),0.72);
    rgb=mix(rgb*(1.-min(water.shade,0.72)),vec3(0.95,0.98,1.)*(0.35+0.85*light),shine);
    // A scene change fades toward the plain wallpaper, then back.
    if p.style.w<1. {
        rgb=mix(scene_at(v.uv,0.),rgb,p.style.w);
    }
    return vec4(rgb,1.);
}
// Downsampling is done only when the wallpaper changes, into cached mipmaps.
@fragment fn downsample(v:Vertex)->@location(0) vec4<f32> {
    return textureSampleLevel(background,linear_sampler,v.uv,0.);
}

// Three analytic snow layers, evaluated per output pixel so flakes move
// smoothly. A flake never gets smaller than about two output pixels across;
// smaller flakes fade instead, and every edge is antialiased.
fn snow(uv:vec2<f32>, time:f32)->f32 {
    var alpha=0.;
    let pixel=max(p.tint.w,1e-5);
    for(var layer=0;layer<3;layer+=1) {
        let depth=f32(layer);
        let scale=22.+depth*18.;
        let speed=p.motion.x*0.14/(1.+depth*0.55);
        let t=time*speed;
        let drift=sin(time*p.motion.x*0.8+depth*2.7)*0.12;
        let q=uv*vec2(p.frame.x,1.)*scale-vec2(t*p.motion.y*scale*0.6+drift,t*scale)+depth*17.3;
        let cell=floor(q);let local=fract(q);
        let place=hash3(cell+depth*61.);
        if place.x>p.frame.z*0.8 {continue;}
        // One output pixel in cell units.
        let px=pixel*scale;
        // Most of a cell is empty sky: leave before the flake's own hash when
        // even the largest, most fluttered flake cannot reach this pixel.
        let reach=max(0.09*(0.45+p.frame.w*1.5),px)+0.5*px;
        let rest=vec2(0.35+place.y*0.3,0.25+place.z*0.5);
        if abs(local.x-rest.x)>0.1+reach || abs(local.y-rest.y)>1.15*reach {continue;}
        let look=hash3(cell+vec2(83.,depth*37.));
        let flutter=sin(time*p.motion.x*(0.6+look.y)+look.z*6.283)*0.10;
        let center=rest+vec2(flutter,0.);
        let radius=(0.025+look.x*0.065)*(0.45+p.frame.w*1.5);
        // A flake too small to cover a pixel keeps its size on screen and
        // fades by area instead, so it cannot pop between pixels.
        let shown=max(radius,px);
        let presence=(radius*radius)/(shown*shown);
        let distance=length((local-center)/vec2(1.,0.85+fract(look.y*7.1+look.z*3.3)*0.3));
        let outer=shown+0.5*px;
        let inner=max(0.,min(shown*0.25,shown-1.5*px));
        let flake=(1.-smoothstep(inner,outer,distance))*presence*(0.85-depth*0.22);
        alpha+=flake;
    }
    return clamp(alpha,0.,1.)*p.atmosphere.x;
}
fn atmosphere_at(uv:vec2<f32>, time:f32)->f32 {
    if p.style.x==3. {return fog(uv,time);}
    if p.style.x==4. {return cloud_shadows(uv,time);}
    return 0.;
}
@fragment fn atmosphere(v:Vertex)->@location(0) f32 {
    let now=atmosphere_at(v.uv,p.frame.y);
    let blend=hourly_blend();
    if blend<=0. {return now;}
    return mix(now,atmosphere_at(v.uv,p.frame.y-PERIOD),blend);
}

// Smooth value noise, three bounded octaves; no evolving textures or readback.
fn noise(q:vec2<f32>)->f32 {
    let cell=floor(q);let f=fract(q);let u=f*f*(3.-2.*f);
    return mix(mix(hash(cell),hash(cell+vec2(1.,0.)),u.x),mix(hash(cell+vec2(0.,1.)),hash(cell+vec2(1.,1.)),u.x),u.y);
}
fn billows(q:vec2<f32>)->f32 {
    return noise(q)*0.57+noise(q*2.03+vec2(13.7,7.1))*0.29+noise(q*4.11+vec2(37.9,19.3))*0.14;
}
fn fog(uv:vec2<f32>, time:f32)->f32 {
    if p.atmosphere.x<=0. || p.atmosphere.y<=0. {return 0.;}
    let t=time*p.motion.x*0.08;
    let q=uv*vec2(p.frame.x*2.4,5.8)-vec2(t*(0.15+p.motion.y*1.4),t*0.12);
    let body=billows(q+vec2(noise(q*0.5)*0.8,0.));
    let top=1.-p.atmosphere.y;
    let height=smoothstep(top,min(1.,top+0.35),uv.y);
    return height*p.atmosphere.x*(0.22+smoothstep(0.20,0.85,body)*0.78);
}

fn cloud_shadows(uv:vec2<f32>, time:f32)->f32 {
    if p.frame.z<=0. || p.atmosphere.x<=0. {return 0.;}
    let t=time*p.motion.x*0.06;
    let q=uv*vec2(p.frame.x*2.1,2.7)-vec2(t*(0.2+p.motion.y),t*0.15);
    let broad=noise(q);
    let detail=billows(q);
    let body=mix(detail,broad,p.atmosphere.z*0.7);
    let edge=0.025+p.atmosphere.z*0.28;
    let threshold=1.-p.frame.z;
    let cover=smoothstep(threshold-edge,threshold+edge,body);
    return cover*p.atmosphere.x*0.75;
}
