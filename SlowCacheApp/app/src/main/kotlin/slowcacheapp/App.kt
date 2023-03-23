import java.time.Instant
import kotlin.random.Random
import kotlin.random.nextULong
import kotlin.system.measureTimeMillis
import kotlin.time.ExperimentalTime
import kotlin.time.measureTimedValue

class App {

}

// contrived, but captures the point.
data class ImportantDataInMemory(
    val timeStamp: ULong,
    val randoData: Any?,
){
    constructor(): this(Random.nextULong(), null) {
    }
}

class ImportantDataSerialized(
    val timeStamp: String,
    val randoData: Any?,
){
    constructor(data: ImportantDataInMemory): this(Instant.ofEpochMilli(data.timeStamp.toLong()).toString(), data.randoData) {
    }
}

class ImportantDataSerializedFast(
    val timeStamp: ULong,
    val randoData: Any?,
){
    constructor(data: ImportantDataInMemory): this(data.timeStamp, data.randoData) {
    }
}
@OptIn(ExperimentalTime::class)
fun main() {
    val timedValue = measureTimedValue {
        genData()
    }
    val timedValueSerialize = measureTimedValue {
        timedValue.value.map { it.key to ImportantDataSerializedFast(it.value) }
    }
    println("took ${timedValue.duration} to generate data.")
    println("took ${timedValueSerialize.duration} to 'serialize' data.")
}

fun genData(): Map<Int, ImportantDataInMemory> {
    return (1..50000000).map { Random.nextInt() to ImportantDataInMemory() }.toMap()
}


